import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { validateWslBuildBinding } from './clearra-build-paths.mjs';

const digest = path => createHash('sha256').update(path).digest('hex').slice(0, 24);
const checkoutId = digest('d:/projects/clearra');
const guestHome = '/home/example person';
const guestSource = `${guestHome}/.local/state/Clearra/wsl/runtime-source/source`;
const sourceId = digest(guestSource);
const guestBuild = '/mnt/c/Users/example/AppData/Local/Clearra/build';
const sessionId = '0123456789abcdef0123456789abcdef';
const local = {
  hostMode: 'local', hostAnchorRoot: 'C:\\Users\\example\\AppData\\Local',
  hostCheckoutRoot: 'D:\\Projects\\Clearra', hostBuildRoot: 'C:\\Users\\example\\AppData\\Local\\Clearra\\build',
  checkoutId, guestBuildRoot: guestBuild, guestHome, guestSourceRoot: guestSource,
};

test('explicit local WSL anchors use the existing LOCALAPPDATA role, not the checkout volume', () => {
  const expected = { checkoutId, sourceId, root: guestBuild, sourceRoot: guestSource };
  assert.notEqual(checkoutId, sourceId, 'host transport and guest compiler-cache identity are different');
  assert.deepEqual(validateWslBuildBinding(local), expected);
  assert.deepEqual(validateWslBuildBinding({ ...local, guestStateHome: '' }), expected);
  assert.deepEqual(validateWslBuildBinding({ ...local,
    hostCheckoutRoot: 'd:/PROJECTS/CLEARRA', hostBuildRoot: 'c:/users/example/appdata/local/clearra/BUILD',
  }), expected);
});

test('explicit hosted WSL anchors require the checkout volume before mapping to a mount', () => {
  const hosted = { ...local, hostMode: 'github-actions', hostAnchorRoot: 'D:\\a\\_temp',
    hostBuildRoot: 'D:\\a\\_temp\\Clearra\\build', guestBuildRoot: '/mnt/d/a/_temp/Clearra/build' };
  assert.equal(validateWslBuildBinding(hosted).root, hosted.guestBuildRoot);
  assert.throws(() => validateWslBuildBinding({ ...hosted, hostCheckoutRoot: 'C:\\Projects\\Clearra' }),
    /share the checkout volume/u);
  assert.throws(() => validateWslBuildBinding({ ...hosted, guestBuildRoot: '/mnt/c/a/_temp/Clearra/build' }),
    /exact Windows mount/u);
});

test('custom WSL XDG state still uses exactly the declared runtime source slot', () => {
  const guestStateHome = '/var/owned-state/example';
  const guestSourceRoot = `${guestStateHome}/Clearra/wsl/runtime-source/source`;
  const custom = { ...local, guestStateHome, guestSourceRoot };
  assert.equal(validateWslBuildBinding(custom).sourceId, digest(guestSourceRoot));
  assert.throws(() => validateWslBuildBinding({ ...custom, guestSourceRoot: guestSource }),
    /exact runtime source slot/u);
  assert.throws(() => validateWslBuildBinding({ ...custom, guestStateHome: '/mnt/c/not-ext4' }),
    /Invalid canonical current-user WSL XDG state/u);
});

test('explicit WSL root and checkout bindings cannot be relocated under a different role or alias', () => {
  for (const proposed of [
    'C:\\other\\Clearra\\build',
    'C:\\Users\\example\\AppData\\Local\\Clearra\\build\\products',
  ]) {
    assert.throws(() => validateWslBuildBinding({ ...local, hostBuildRoot: proposed }), /exact host cache role/u);
  }
  assert.throws(() => validateWslBuildBinding({ ...local,
    hostBuildRoot: 'C:\\Users\\example\\AppData\\Local\\Clearra\\scratch\\..\\build',
  }), /Invalid canonical Windows/u);
  assert.throws(() => validateWslBuildBinding({ ...local, guestBuildRoot: `${guestBuild}/experiments` }),
    /exact Windows mount/u);
  assert.throws(() => validateWslBuildBinding({ ...local, guestSourceRoot: `${guestSource}/nested` }),
    /exact runtime source slot/u);
  assert.throws(() => validateWslBuildBinding({ ...local, guestSourceRoot: `${guestSource}/../source` }),
    /exact runtime source slot/u);
  assert.throws(() => validateWslBuildBinding({ ...local,
    guestSourceRoot: `${guestHome}/.local/share/Clearra/workspaces/${checkoutId}/source`,
  }), /exact runtime source slot/u);
  assert.throws(() => validateWslBuildBinding({ ...local, guestHome: '/home/other' }), /exact runtime source slot/u);
  for (const home of ['/mnt/c/Users/example', '/home/other/../example', '/root', '/home/alias/']) {
    assert.throws(() => validateWslBuildBinding({ ...local, guestHome: home }), /Invalid canonical current-user/u);
  }
  assert.throws(() => validateWslBuildBinding({ ...local, checkoutId: 'e'.repeat(24) }), /checkout identity mismatch/u);
});

test('explicit WSL experiment and product outputs retain their registered owner roles', () => {
  const experimentRoot = `${guestBuild}/experiments/${sourceId}/current`;
  const experiment = { purpose: 'experiment', sourceId, sessionId,
    transactionRoot: experimentRoot, cargoTarget: `${experimentRoot}/cargo-target` };
  const accepted = validateWslBuildBinding({ ...local, owner: experiment });
  assert.equal(accepted.transaction, experimentRoot);
  assert.equal(accepted.cargoTarget, experiment.cargoTarget);
  assert.throws(() => validateWslBuildBinding({ ...local, owner: { ...experiment, sourceId: checkoutId } }),
    /owner identity mismatch/u);
  for (const cargoTarget of [`${accepted.cargoTarget}/shadow`, `${guestHome}/.cache/Clearra/build/cargo-target`]) {
    assert.throws(() => validateWslBuildBinding({ ...local, owner: { ...experiment, cargoTarget } }),
      /exact transaction role/u);
  }
  const productRoot = `${guestBuild}/products/20261120T000000000Z-${sessionId}`;
  const product = { ...experiment, purpose: 'product', createdUtc: '2026-11-20T00:00:00.000Z',
    transactionRoot: productRoot, cargoTarget: `${productRoot}/cargo-target` };
  assert.equal(validateWslBuildBinding({ ...local, owner: product }).transaction, productRoot);
  assert.throws(() => validateWslBuildBinding({ ...local, owner: { ...product, createdUtc: '2026-11-21T00:00:00.000Z' } }),
    /exact transaction role/u);
  assert.throws(() => validateWslBuildBinding({ ...local, owner: { ...product, createdUtc: '2026-11-31T00:00:00.000Z' } }),
    /exact UTC timestamp/u);
});

test('existing build path CLI does not accept prospective WSL bindings as options', () => {
  const script = fileURLToPath(new URL('./clearra-build-paths.mjs', import.meta.url));
  const result = spawnSync(process.execPath, [script, '--guest-build-root', guestBuild],
    { encoding: 'utf8', windowsHide: true });
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Unknown build path option/u);
});

test('direct build path CLI does not silently accept an incomplete option', () => {
  const script = fileURLToPath(new URL('./clearra-build-paths.mjs', import.meta.url));
  const result = spawnSync(process.execPath, [script, '--field'], {
    encoding: 'utf8', windowsHide: true,
  });
  assert.equal(result.status, 1, result.stderr || result.error?.message);
  assert.equal(result.stdout, '');
  assert.match(result.stderr, /Build path options require values/u);
});
