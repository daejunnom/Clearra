import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, readdir, rm, stat, writeFile, symlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { acquireBuildOwner } from './clearra-build-owner.mjs';
import { assertBuildPathWithin, assertCargoOutputArguments, assertManagedBuildTransaction, buildPathIdentity, canonicalBuildRoot, nativeBuildPath } from './clearra-build-policy.mjs';

function cleanBuildEnvironment() {
  const environment = { ...process.env };
  for (const key of Object.keys(environment)) {
    if (/^CLEARRA_BUILD_/u.test(key) || ['CARGO_TARGET_DIR', 'CARGO_INCREMENTAL', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER'].includes(key)) {
      delete environment[key];
    }
  }
  return environment;
}

async function fixture(t) {
  const temporary = await mkdtemp(join(tmpdir(), 'clearra-build-policy-test-'));
  t.after(async () => {
    // Only the exact directory created by this fixture can be removed.
    assert.equal(dirname(resolve(temporary)), resolve(tmpdir()));
    assert.ok(basename(temporary).startsWith('clearra-build-policy-test-'));
    await rm(temporary, { recursive: true, force: true });
  });
  const sourceRoot = join(temporary, 'source');
  await mkdir(sourceRoot);
  await writeFile(join(sourceRoot, 'Cargo.toml'), '[workspace]\n');
  const environment = { LOCALAPPDATA: join(temporary, 'cache'), XDG_CACHE_HOME: join(temporary, 'cache'), HOME: temporary,
    GITHUB_ACTIONS: '', RUNNER_TEMP: '', GITHUB_WORKSPACE: '' };
  return { temporary, sourceRoot, environment };
}

test('path identity is stable across Windows, WSL mounts, and WSL-native sources', () => {
  assert.equal(buildPathIdentity('C:\\Users\\Example\\AppData\\Local\\Clearra\\build'), buildPathIdentity('/mnt/c/Users/Example/AppData/Local/Clearra/build'));
  assert.equal(nativeBuildPath('/mnt/c/Users/Example/AppData/Local/Clearra/build', 'win32'), 'C:\\Users\\Example\\AppData\\Local\\Clearra\\build');
  assert.equal(nativeBuildPath('C:\\Users\\Example\\AppData\\Local\\Clearra\\build', 'linux'), '/mnt/c/Users/Example/AppData/Local/Clearra/build');
  assert.equal(buildPathIdentity('/home/clearra/workspaces/source'), '/home/clearra/workspaces/source');
  assert.throws(() => assertBuildPathWithin('/tmp/clearra-build-other', '/tmp/clearra-build'));
  assert.throws(() => assertBuildPathWithin('/tmp/clearra-build/../escape', '/tmp/clearra-build'));
});

test('GitHub Windows builds keep the canonical root on the checkout volume', () => {
  const hosted = { GITHUB_ACTIONS: 'true', GITHUB_WORKSPACE: 'D:\\a\\Clearra\\Clearra',
    RUNNER_TEMP: 'D:\\a\\_temp', LOCALAPPDATA: 'C:\\Users\\runneradmin\\AppData\\Local' };
  assert.equal(canonicalBuildRoot(hosted, 'win32'), 'D:\\a\\_temp\\Clearra\\build');
  assert.throws(() => canonicalBuildRoot({ ...hosted, RUNNER_TEMP: 'C:\\runner-temp' }, 'win32'), /share the checkout volume/u);
  assert.equal(canonicalBuildRoot({ LOCALAPPDATA: hosted.LOCALAPPDATA }, 'win32'),
    'C:\\Users\\runneradmin\\AppData\\Local\\Clearra\\build');
});

test('Windows native compiler guard preserves argv beyond the cmd limit', { skip: process.platform !== 'win32' }, async t => {
  const options = await fixture(t);
  const owner = await acquireBuildOwner(options);
  try {
    assert.ok(owner.environment.RUSTC_WRAPPER.endsWith('.exe'));
    const payload = 'quoted"한글\\path '.repeat(750);
    const code = `if (process.argv[1] !== ${JSON.stringify(payload)}) process.exit(9)`;
    const result = spawnSync(owner.environment.RUSTC_WRAPPER, [process.execPath, '-e', code, payload], {
      env: { ...process.env, ...owner.environment }, encoding: 'utf8', windowsHide: true,
    });
    assert.equal(result.status, 0, result.stderr || result.error?.message);
  } finally { await owner.finish(true); }
});

test('outside target and compiler overrides fail before directory creation', async t => {
  const options = await fixture(t);
  for (const key of ['CARGO_TARGET_DIR', 'CARGO_BUILD_TARGET_DIR', 'CARGO_BUILD_BUILD_DIR', 'CLEARRA_WSL_CARGO_TARGET_DIR', 'CLEARRA_CORE_C_BUILD_DIR', 'RUSTC_WRAPPER']) {
    await assert.rejects(acquireBuildOwner({ ...options, environment: { ...options.environment, [key]: join(options.temporary, 'outside') } }), /override|wrapper/iu);
  }
  await assert.rejects(stat(options.environment.LOCALAPPDATA), { code: 'ENOENT' });
});

test('experimental source purpose reuses only a complete compiler cache with provenance', async t => {
  const options = await fixture(t);
  const first = await acquireBuildOwner(options);
  const target = first.transaction.cargo_target_dir;
  await writeFile(join(target, 'old-hashed-dependency.rlib'), 'old');
  assert.equal(first.environment.CARGO_INCREMENTAL, '1');
  assert.equal(assertManagedBuildTransaction({ environment: first.environment, sourceRoot: options.sourceRoot }).cargoTarget, target);
  await first.finish(true);
  const second = await acquireBuildOwner(options);
  assert.equal(second.transaction.transaction_root, first.transaction.transaction_root);
  assert.notEqual(second.transaction.session_id, first.transaction.session_id);
  assert.equal(await readFile(join(target, 'old-hashed-dependency.rlib'), 'utf8'), 'old');
  assert.equal(second.transaction.incremental_seed_session_id, first.transaction.session_id);
  assert.equal(second.transaction.incremental_seed_snapshot_sha256, first.transaction.compiler_snapshot_sha256);
  assert.deepEqual(await readdir(resolve(target, '../..')), ['current']);
  await second.finish(false);
  const third = await acquireBuildOwner(options);
  await assert.rejects(stat(join(target, 'old-hashed-dependency.rlib')), { code: 'ENOENT' });
  assert.equal(third.transaction.incremental_seed_session_id, null);
  await third.finish(false);
});

test('a moving compiler input cannot seal a reusable experiment snapshot', async t => {
  const options = await fixture(t);
  const owner = await acquireBuildOwner(options);
  await writeFile(join(options.sourceRoot, 'Cargo.toml'), '[workspace]\n# changed during build\n');
  await assert.rejects(owner.finish(true), /inputs changed/iu);
  const marker = JSON.parse(await readFile(join(owner.transaction.transaction_root, '.clearra-build-transaction.json'), 'utf8'));
  assert.equal(marker.status, 'failed');
  const next = await acquireBuildOwner(options);
  assert.equal(next.transaction.incremental_seed_session_id, null);
  await next.finish(false);
});

test('independent owners cannot steal a purpose; nested owner reuses it', async t => {
  const options = await fixture(t);
  const owner = await acquireBuildOwner(options);
  await assert.rejects(acquireBuildOwner(options), /already has an owner/u);
  const nested = await acquireBuildOwner({ ...options, environment: owner.environment });
  await nested.finish(true);
  assert.equal(assertManagedBuildTransaction({ environment: owner.environment }).session_id, owner.transaction.session_id);
  await owner.finish(true);
  assert.throws(() => assertManagedBuildTransaction({ environment: owner.environment }), /active/u);
});

test('source, lease and output mismatches cannot compile', async t => {
  const options = await fixture(t);
  const owner = await acquireBuildOwner(options);
  assert.throws(() => assertManagedBuildTransaction({ environment: { ...owner.environment, CLEARRA_BUILD_SOURCE_ID: '0'.repeat(24) } }), /binding/u);
  assert.throws(() => assertCargoOutputArguments(['--out-dir', options.temporary], owner.transaction.transaction_root), /outside/u);
  assert.throws(() => assertCargoOutputArguments(['--emit=metadata=' + join(options.temporary, 'escape.rmeta')], owner.transaction.transaction_root), /outside/u);
  assert.throws(() => assertCargoOutputArguments(['--emit', 'metadata=' + join(options.temporary, 'escape.rmeta')], owner.transaction.transaction_root), /outside/u);
  assert.throws(() => assertCargoOutputArguments(['-o' + join(options.temporary, 'escape.rmeta')], owner.transaction.transaction_root), /outside/u);
  assert.doesNotThrow(() => assertCargoOutputArguments(['--out-dir', owner.transaction.cargo_target_dir, '--emit=dep-info,metadata'], owner.transaction.transaction_root));
  await owner.finish(false);
});

test('product keeps five complete generations and zero failed generations', async t => {
  const options = await fixture(t);
  let productRoot;
  let crossHostGeneration;
  for (let index = 0; index < 7; index += 1) {
    const owner = await acquireBuildOwner({ ...options, purpose: 'product' });
    assert.equal(owner.environment.CARGO_INCREMENTAL, '0');
    assert.equal(owner.transaction.incremental_seed_session_id, null);
    productRoot = resolve(owner.transaction.transaction_root, '..');
    await writeFile(join(owner.transaction.cargo_target_dir, 'product'), String(index));
    await owner.finish(true);
    if (index === 0 && process.platform === 'win32') {
      crossHostGeneration = owner.transaction.transaction_root;
      const markerPath = join(crossHostGeneration, '.clearra-build-transaction.json');
      const marker = JSON.parse(await readFile(markerPath, 'utf8'));
      const toWslMount = value => {
        const normalized = resolve(value).replaceAll('\\', '/');
        return `/mnt/${normalized[0].toLowerCase()}/${normalized.slice(3)}`;
      };
      marker.source_root = toWslMount(marker.source_root);
      marker.transaction_root = toWslMount(marker.transaction_root);
      marker.cargo_target_dir = toWslMount(marker.cargo_target_dir);
      await writeFile(markerPath, `${JSON.stringify(marker)}\n`);
    }
  }
  assert.equal((await readdir(productRoot)).length, 5);
  if (crossHostGeneration) await assert.rejects(stat(crossHostGeneration), { code: 'ENOENT' });
  const failed = await acquireBuildOwner({ ...options, purpose: 'product' });
  await assert.rejects(acquireBuildOwner({ ...options, purpose: 'product' }), /independent product builds/u);
  await failed.finish(false);
  await assert.rejects(stat(failed.transaction.transaction_root), { code: 'ENOENT' });
  assert.equal((await readdir(productRoot)).length, 5);
  const experiment = await acquireBuildOwner(options);
  await experiment.finish(true);
  assert.equal((await readdir(productRoot)).length, 5);
});

test('unknown current directory and junctions are never replaced', async t => {
  const options = await fixture(t);
  const owner = await acquireBuildOwner(options);
  await owner.finish(true);
  await rm(join(owner.transaction.transaction_root, '.clearra-build-transaction.json'));
  await writeFile(join(owner.transaction.transaction_root, 'user-file'), 'preserve');
  await assert.rejects(acquireBuildOwner(options), /without ownership/u);
  assert.equal(await readFile(join(owner.transaction.transaction_root, 'user-file'), 'utf8'), 'preserve');
  const link = join(options.temporary, 'link');
  await symlink(options.sourceRoot, link, process.platform === 'win32' ? 'junction' : 'dir');
  assert.throws(() => assertCargoOutputArguments(['-o', join(link, 'escape')], options.temporary), /symlink|junction/u);
});

test('active generation without a lease and nested aliases fail closed', async t => {
  const options = await fixture(t);
  const owner = await acquireBuildOwner(options);
  assert.throws(() => assertManagedBuildTransaction({ environment: { ...owner.environment, CARGO_BUILD_TARGET_DIR: owner.environment.CARGO_TARGET_DIR } }), /override/u);
  const lease = join(owner.environment.CLEARRA_BUILD_ROOT, '.leases', `experiment-${owner.transaction.source_id}.lock`);
  await rm(lease);
  await assert.rejects(acquireBuildOwner(options), /active experimental/u);
  assert.equal(JSON.parse(await readFile(join(owner.transaction.transaction_root, '.clearra-build-transaction.json'), 'utf8')).status, 'active');
});

test('Node and PowerShell reuse the same lease without nested completion', { skip: process.platform !== 'win32' }, async t => {
  const options = await fixture(t);
  const owner = await acquireBuildOwner(options);
  const helper = fileURLToPath(new URL('../lib/clearra-path-helpers.ps1', import.meta.url));
  const paths = fileURLToPath(new URL('./clearra-build-paths.mjs', import.meta.url));
  const childEnv = { ...cleanBuildEnvironment(), ...owner.environment, CLEARRA_TEST_HELPER: helper, CLEARRA_TEST_PATHS: paths };
  const nested = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-Command',
    "$ErrorActionPreference='Stop'; . $env:CLEARRA_TEST_HELPER; Ensure-ClearraBuildArtifactCache -RepositoryRoot $env:CLEARRA_BUILD_SOURCE_ROOT; if(Test-ClearraBuildTransactionOwner){throw 'nested became owner'}; Get-ClearraCargoTargetDir; Exit-ClearraBuildArtifactCacheUsage"], { env: childEnv, encoding: 'utf8', windowsHide: true });
  assert.equal(nested.status, 0, nested.stderr + nested.stdout);
  assert.equal(buildPathIdentity(nested.stdout.trim()), buildPathIdentity(owner.transaction.cargo_target_dir));
  await owner.finish(true);
  const environment = { ...cleanBuildEnvironment(), ...options.environment, CLEARRA_TEST_SOURCE: options.sourceRoot, CLEARRA_TEST_HELPER: helper, CLEARRA_TEST_PATHS: paths };
  const reverse = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-Command',
    "$ErrorActionPreference='Stop'; . $env:CLEARRA_TEST_HELPER; try { Ensure-ClearraBuildArtifactCache -RepositoryRoot $env:CLEARRA_TEST_SOURCE; node $env:CLEARRA_TEST_PATHS --field cargo-target; if($LASTEXITCODE -ne 0){throw 'Node rejected PS owner'}; Complete-ClearraBuildTransaction } finally { Exit-ClearraBuildArtifactCacheUsage }"], { env: environment, encoding: 'utf8', windowsHide: true });
  assert.equal(reverse.status, 0, reverse.stderr + reverse.stdout);
  assert.equal(buildPathIdentity(reverse.stdout.trim()), buildPathIdentity(owner.transaction.cargo_target_dir));
  const product = await acquireBuildOwner({ ...options, purpose: 'product' });
  const productConsumer = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-Command',
    "$ErrorActionPreference='Stop'; . $env:CLEARRA_TEST_HELPER; Ensure-ClearraBuildArtifactCache -RepositoryRoot $env:CLEARRA_BUILD_SOURCE_ROOT; if(Test-ClearraBuildTransactionOwner){throw 'nested product became owner'}; Get-ClearraCargoTargetDir; Exit-ClearraBuildArtifactCacheUsage"], {
    env: { ...cleanBuildEnvironment(), ...product.environment, CLEARRA_TEST_HELPER: helper }, encoding: 'utf8', windowsHide: true,
  });
  assert.equal(productConsumer.status, 0, productConsumer.stderr + productConsumer.stdout);
  await product.finish(true);
});

test('bare Cargo writes under the declared build root without a mandatory wrapper', async t => {
  const options = await fixture(t);
  await mkdir(join(options.sourceRoot, '.cargo'));
  await mkdir(join(options.sourceRoot, 'src'));
  await writeFile(join(options.sourceRoot, 'Cargo.toml'), '[package]\nname="clearra_path_guard_fixture"\nversion="0.0.0"\nedition="2021"\n');
  await writeFile(join(options.sourceRoot, 'src', 'lib.rs'), 'pub fn fixture() {}\n');
  const config = await readFile(new URL('../../.cargo/config.toml', import.meta.url), 'utf8');
  await writeFile(join(options.sourceRoot, '.cargo', 'config.toml'), config);
  const environment = { ...process.env };
  delete environment.RUSTC_WRAPPER;
  delete environment.CARGO_TARGET_DIR;
  const result = spawnSync('cargo', ['check', '--offline'], { cwd: options.sourceRoot, env: environment, encoding: 'utf8', windowsHide: true });
  if (result.error?.code === 'ENOENT') { t.skip('Cargo is not installed on this policy-only host'); return; }
  assert.equal(result.status, 0, result.stderr);
  assert.ok((await stat(join(options.sourceRoot, 'build', 'cargo', 'default'))).isDirectory());
  await assert.rejects(stat(join(options.sourceRoot, 'target')), { code: 'ENOENT' });
});
