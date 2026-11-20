import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtemp, mkdir, rm, rmdir, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { setTimeout as wait } from 'node:timers/promises';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { BUILD_STORAGE_POLICY, checkBuildStorage, monitorBuildStorage } from './clearra-build-capacity.mjs';
import { BUILD_TRANSACTION_MARKER, buildSourceId } from './clearra-build-policy.mjs';

async function fixture(t) {
  const workspace = await mkdtemp(join(tmpdir(), 'clearra-build-capacity-test-'));
  t.after(async () => {
    assert.equal(dirname(resolve(workspace)), resolve(tmpdir()));
    assert.ok(basename(workspace).startsWith('clearra-build-capacity-test-'));
    await rm(workspace, { recursive: true, force: true });
  });
  return { root: join(workspace, 'catalog'), source: join(workspace, 'source') };
}

async function makeSlot({ root, source }, index, { purpose = 'product', status = 'complete', size = 1 } = {}) {
  const sourceId = buildSourceId(source);
  const stamp = new Date(Date.UTC(2026, 10, 20, 0, 0, index)).toISOString();
  const session = (index + 1).toString(16).padStart(32, '0');
  const path = purpose === 'product'
    ? join(root, 'products', `${stamp.replace(/[-:.]/gu, '')}-${session}`)
    : join(root, 'experiments', sourceId, 'current');
  const target = join(path, 'cargo-target');
  const marker = JSON.stringify({
    schema_version: 3, purpose, source_root: source, source_id: sourceId, session_id: session,
    transaction_root: path, cargo_target_dir: target, owner_pid: process.pid, status,
    created_utc: stamp, completed_utc: status === 'complete' ? stamp : null,
  });
  await mkdir(target, { recursive: true });
  await writeFile(join(path, BUILD_TRANSACTION_MARKER), marker);
  await writeFile(join(target, 'compiled'), Buffer.alloc(size));
  return { path, target, bytes: Buffer.byteLength(marker) + size };
}

test('build storage counts logical bytes and uses exact slot/catalog boundaries', async t => {
  const f = await fixture(t);
  const first = await makeSlot(f, 0, { status: 'active', size: 127 });
  const second = await makeSlot(f, 1, { size: 31 });
  const exact = { ...BUILD_STORAGE_POLICY, slotBytes: Math.max(first.bytes, second.bytes),
    catalogBytes: first.bytes + second.bytes };
  const observed = await checkBuildStorage(f.root, { slot: first.path, limits: exact });
  assert.equal(observed.requestedBytes, first.bytes);
  assert.equal(observed.catalogBytes, first.bytes + second.bytes);
  assert.equal(observed.active, 1);
  assert.equal(observed.completed, 1);
  await assert.rejects(checkBuildStorage(f.root, { slot: first.path, limits: { ...exact, slotBytes: exact.slotBytes - 1 } }),
    { code: 'E_CLEARRA_BUILD_STORAGE_LIMIT', message: /build slot exceeds/u });
  await assert.rejects(checkBuildStorage(f.root, { slot: first.path, limits: { ...exact, catalogBytes: exact.catalogBytes - 1 } }),
    { code: 'E_CLEARRA_BUILD_STORAGE_LIMIT', message: /build catalog exceeds/u });
  assert.equal((await checkBuildStorage(f.root, { slot: first.path, phase: 'complete', limits: exact })).completed, 2);
});

test('build storage checks admission and reuses the exact prior experiment at four retained slots', async t => {
  const f = await fixture(t);
  const current = await makeSlot(f, 0, { purpose: 'experiment' });
  for (let index = 1; index <= 3; index++) await makeSlot(f, index);
  const retained = await checkBuildStorage(f.root);
  assert.equal(retained.completed, 4);
  const reuse = await checkBuildStorage(f.root, { slot: current.path, phase: 'admit' });
  assert.equal(reuse.active, 1);
  assert.equal(reuse.completed, 3);
  const other = join(f.root, 'experiments', buildSourceId(join(f.source, 'other')), 'current');
  await assert.rejects(checkBuildStorage(f.root, { slot: other, phase: 'admit' }), { code: 'E_CLEARRA_BUILD_STORAGE_LIMIT' });
  await assert.rejects(checkBuildStorage(f.root, { slot: join(f.root, 'historical'), phase: 'admit' }), /exact managed build slot/u);
  assert.equal((await checkBuildStorage(f.root)).completed, 4);
});

test('build storage rejects unknown ownership, nested links, and incomplete bounded walks', async t => {
  const f = await fixture(t);
  const slot = await makeSlot(f, 0, { status: 'active' });
  const check = () => checkBuildStorage(f.root, { slot: slot.path });
  await assert.rejects(checkBuildStorage(f.root, { slot: slot.path, limits: { ...BUILD_STORAGE_POLICY, maxEntries: 2 } }),
    { code: 'E_CLEARRA_BUILD_STORAGE_INCOMPLETE' });
  await assert.rejects(checkBuildStorage(f.root, { slot: slot.path, limits: { ...BUILD_STORAGE_POLICY, maxDepth: 3 } }),
    { code: 'E_CLEARRA_BUILD_STORAGE_INCOMPLETE' });
  const legacy = join(f.root, 'legacy');
  await mkdir(legacy);
  await assert.rejects(check(), /unknown entry/u);
  await rmdir(legacy);
  await mkdir(f.source);
  await symlink(f.source, join(slot.target, 'link'), process.platform === 'win32' ? 'junction' : 'dir');
  await assert.rejects(check(), /refuses a nested symlink or junction/u);
});

test('build storage monitor observes one interval at a time and stops on first failure', async () => {
  let attempts = 0;
  let errors = 0;
  const stop = monitorBuildStorage(async () => {
    attempts++;
    await wait(6);
    throw new Error('fixture capacity limit');
  }, error => {
    errors++;
    assert.equal(error.message, 'fixture capacity limit');
  }, { sampleMs: 1 });
  await wait(50);
  await stop();
  assert.equal(attempts, 1);
  assert.equal(errors, 1);
});

test('compiler capacity CLI is bound to the shared canonical catalog', async t => {
  const f = await fixture(t);
  const local = join(dirname(f.root), 'profile');
  const canonical = join(local, 'Clearra', 'build');
  const run = root => spawnSync(process.execPath, [
    fileURLToPath(new URL('./clearra-build-capacity.mjs', import.meta.url)), 'check', '--root', root, '--phase', 'admit',
  ], { env: { ...process.env, LOCALAPPDATA: local, XDG_CACHE_HOME: local,
    GITHUB_ACTIONS: '', WSL_DISTRO_NAME: '', WSL_INTEROP: '' }, encoding: 'utf8', windowsHide: true });
  const rejected = run(f.root);
  assert.equal(rejected.status, 1);
  assert.equal(JSON.parse(rejected.stderr).code, 'E_CLEARRA_BUILD_STORAGE_UNOWNED');
  const accepted = run(canonical);
  assert.equal(accepted.status, 0, accepted.stderr);
  assert.deepEqual(JSON.parse(accepted.stdout), {
    phase: 'admit', catalogBytes: 0, requestedBytes: 0, completed: 0, active: 0, visited: 1,
  });
});
