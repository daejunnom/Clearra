import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { prepareComputeAccelerators, requiresV081Accelerators } from '../scripts/provision-v081-accelerators.mjs';

const PROFILES = ['srs', 'srs-plus', 'srs-x', 'jstris-180', 'no-kick'];
const CATALOG_ID = 'a'.repeat(64);
const GENERATION_ID = 'b'.repeat(64);
const EXECUTABLE = process.platform === 'win32' ? 'C:\\clearra\\clearra.exe' : '/usr/local/bin/clearra';

function fakeCli(calls, change = () => undefined) {
  return (_executable, arguments_) => {
    const [command, action, , profile] = arguments_;
    assert.ok(['legal-board', 'reachability-pack'].includes(command));
    assert.ok(PROFILES.includes(profile));
    assert.equal(arguments_.at(-2), '--format');
    assert.equal(arguments_.at(-1), 'json');
    calls.push(`${command}:${action}:${profile}`);
    const base = { action, profile, qualified: true, catalog_identity: CATALOG_ID };
    const value = action === 'check'
      ? { ...base, catalog_status: 'ready', network_used: false,
        generation_identity: GENERATION_ID, compressed_bytes: 1024 }
      : { ...base, installed: true, validation: 'ready',
        installed_generation_identity: GENERATION_ID, installed_payload_bytes: 1024 };
    return change(value, command, action, profile) ?? value;
  };
}

test('v0.8.1 and later require immutable compute data; v0.8.0 does not', () => {
  assert.equal(requiresV081Accelerators('0.8.0'), false);
  assert.equal(requiresV081Accelerators('0.8.1'), true);
  assert.equal(requiresV081Accelerators('0.9.0'), true);
  assert.equal(requiresV081Accelerators('1.0.0'), true);
  assert.throws(() => requiresV081Accelerators('unbound'), /semver/u);
  const calls = [];
  assert.deepEqual(prepareComputeAccelerators({ mode: 'provision', version: '0.8.0',
    executable: EXECUTABLE, root: join(tmpdir(), 'unused-clearra-compute-assets'), invoke: fakeCli(calls) }),
  { required: false, profiles: 0 });
  assert.deepEqual(calls, []);
});

test('accepted image checks all ten signed slots before downloading and verifies each install', async t => {
  const root = await mkdtemp(join(tmpdir(), 'clearra-compute-assets-test-'));
  t.after(() => rm(root, { recursive: true, force: true }));
  const calls = [];
  const invoke = fakeCli(calls);
  assert.deepEqual(prepareComputeAccelerators({ mode: 'provision', version: '0.8.1',
    executable: EXECUTABLE, root, invoke }), { required: true, profiles: 5 });
  assert.equal(calls.length, 30);
  assert.ok(calls.slice(0, 10).every(value => value.includes(':check:')));
  assert.ok(calls.slice(10, 20).every(value => value.includes(':download:')));
  assert.ok(calls.slice(20).every(value => value.includes(':status:')));
  const verifyCalls = [];
  prepareComputeAccelerators({ mode: 'verify', version: '0.8.1', executable: EXECUTABLE,
    root, invoke: fakeCli(verifyCalls) });
  assert.equal(verifyCalls.length, 20);
  assert.ok(verifyCalls.every(value => !value.includes(':download:')));
});

test('an unqualified profile or mismatched installed generation fails closed', () => {
  const root = join(tmpdir(), 'unused-clearra-compute-assets');
  const calls = [];
  assert.throws(() => prepareComputeAccelerators({ mode: 'provision', version: '0.8.1',
    executable: EXECUTABLE, root, invoke: fakeCli(calls, (value, _command, action, profile) =>
      action === 'check' && profile === 'srs-x' ? { ...value, qualified: false } : undefined) }),
  /not qualified/u);
  assert.ok(calls.every(value => value.includes(':check:')));

  const verifyCalls = [];
  assert.throws(() => prepareComputeAccelerators({ mode: 'verify', version: '0.8.1',
    executable: EXECUTABLE, root, invoke: fakeCli(verifyCalls, (value, _command, action, profile) =>
      action === 'status' && profile === 'srs-plus'
        ? { ...value, installed_generation_identity: 'c'.repeat(64) } : undefined) }),
  /not bound to the signed catalog/u);
});

test('accepted Dockerfile provisions before dropping privileges and verifies as node', async () => {
  const dockerfile = await readFile(new URL('../Dockerfile.accepted-job-service', import.meta.url), 'utf8');
  const provision = dockerfile.indexOf('provision-v081-accelerators.mjs provision');
  const user = dockerfile.indexOf('USER node', provision);
  const verify = dockerfile.indexOf('provision-v081-accelerators.mjs verify', user);
  assert.ok(provision > 0 && user > provision && verify > user);
  assert.match(dockerfile, /CLEARRA_LEGAL_BOARD_DIRECTORY=\/opt\/clearra\/accelerators\/legal-board/u);
  assert.match(dockerfile, /CLEARRA_CONDITIONED_REACHABILITY_DIRECTORY=\/opt\/clearra\/accelerators\/conditioned-reachability/u);
  assert.match(dockerfile, /chmod -R a-w \/opt\/clearra\/accelerators/u);
});
