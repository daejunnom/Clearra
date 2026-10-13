import assert from 'node:assert/strict';
import { access, mkdir, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { buildSourceId } from './clearra-build-policy.mjs';
import { acquireManagedTransientDirectory, assertManagedTransientSlotsRetired } from './managed-transient-directory.mjs';

const cacheBase = process.platform === 'win32'
  ? process.env.LOCALAPPDATA || process.env.TEMP || resolve(process.env.USERPROFILE || '.', 'AppData', 'Local')
  : process.env.XDG_CACHE_HOME || resolve(process.env.HOME || '.', '.cache');
const checkout = fileURLToPath(new URL('../..', import.meta.url));
const baseSlot = resolve(cacheBase, 'Clearra', 'benchmark-runtime', 'managed-slot-contract',
  buildSourceId(checkout), `${process.platform}-${process.arch}`);
const suite = await acquireManagedTransientDirectory(baseSlot);
const slot = resolve(suite.path, 'normal');

try {
  const guardedRoot = resolve(suite.path, 'outer-build');
  await mkdir(guardedRoot);
  const guardedLease = await acquireManagedTransientDirectory(resolve(guardedRoot, 'wasm-stage'));
  await assert.rejects(assertManagedTransientSlotsRetired(guardedRoot), /unresolved build scratch lease/);
  await guardedLease.release({ remove: false });
  await assertManagedTransientSlotsRetired(guardedRoot);

  const first = await acquireManagedTransientDirectory(slot);
  await assert.rejects(first.handoffToParent(), /verified parent owner/);
  await writeFile(resolve(first.path, 'stale.txt'), 'stale', 'utf8');
  await first.release({ remove: false });

  const second = await acquireManagedTransientDirectory(slot);
  try {
    await assert.rejects(access(resolve(second.path, 'stale.txt')));
    await assert.rejects(
      acquireManagedTransientDirectory(slot),
      /already active/
    );
  } finally {
    await second.release();
  }

  const unknown = resolve(suite.path, 'unknown');
  await mkdir(unknown);
  await writeFile(resolve(unknown, 'keep.txt'), 'not-owned');
  await assert.rejects(acquireManagedTransientDirectory(unknown), /unknown or interrupted ownership/);
  assert.equal(await readFile(resolve(unknown, 'keep.txt'), 'utf8'), 'not-owned');

  const stale = resolve(suite.path, 'stale');
  await writeFile(`${stale}.lock`, JSON.stringify({ schema_version: 1, pid: 2147483647 }));
  await assert.rejects(acquireManagedTransientDirectory(stale), /explicit reviewed recovery/);
  assert.equal(JSON.parse(await readFile(`${stale}.lock`, 'utf8')).pid, 2147483647);

  const changed = resolve(suite.path, 'changed');
  const changedLease = await acquireManagedTransientDirectory(changed);
  await writeFile(resolve(changed, 'keep.txt'), 'keep');
  await writeFile(`${changed}.lock`, 'null');
  await assert.rejects(changedLease.release(), /ownership changed/);
  assert.equal(await readFile(resolve(changed, 'keep.txt'), 'utf8'), 'keep');
  assert.equal(await readFile(`${changed}.lock`, 'utf8'), 'null');

  const foreign = resolve(suite.path, 'foreign');
  const foreignLease = await acquireManagedTransientDirectory(foreign);
  await foreignLease.release({ remove: false });
  const foreignRecord = JSON.parse(await readFile(`${foreign}.owner.json`, 'utf8'));
  await writeFile(`${foreign}.owner.json`, JSON.stringify({ ...foreignRecord, arch: 'other-arch' }));
  await assert.rejects(acquireManagedTransientDirectory(foreign), /unknown or interrupted ownership/);

  const target = resolve(suite.path, 'protected-target');
  await mkdir(target);
  await writeFile(resolve(target, 'keep.txt'), 'keep');
  const linked = resolve(suite.path, 'linked');
  const linkedLease = await acquireManagedTransientDirectory(linked);
  let linkedFixture = false;
  try {
    await symlink(target, resolve(linked, 'escape'), process.platform === 'win32' ? 'junction' : 'dir');
    linkedFixture = true;
  } catch (error) {
    if (!['EPERM', 'EACCES', 'ENOSYS'].includes(error.code)) throw error;
  }
  if (linkedFixture) {
    await assert.rejects(linkedLease.release(), /symlink or junction/);
    assert.equal(await readFile(resolve(target, 'keep.txt'), 'utf8'), 'keep');
    // Only this deliberately created test link is removed. Never follow it.
    await rm(resolve(linked, 'escape'));
  } else {
    await linkedLease.release();
  }

  console.log('managed_transient_directory_contract=passed lifecycle_cases=8');
} finally {
  // The suite created all fixtures above and owns the exact fixed test root.
  await suite.release();
}
