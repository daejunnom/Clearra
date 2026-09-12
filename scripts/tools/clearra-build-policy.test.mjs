import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, readdir, rm, stat, writeFile, symlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { acquireBuildOwner } from './clearra-build-owner.mjs';
import { assertBuildPathWithin, assertCargoOutputArguments, assertManagedBuildTransaction, buildPathIdentity } from './clearra-build-policy.mjs';

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
  const environment = { LOCALAPPDATA: join(temporary, 'cache'), XDG_CACHE_HOME: join(temporary, 'cache'), HOME: temporary };
  return { temporary, sourceRoot, environment };
}

test('one physical path identity matches Windows and its WSL mount', () => {
  assert.equal(buildPathIdentity('C:\\Users\\Example\\AppData\\Local\\Clearra\\build'), buildPathIdentity('/mnt/c/Users/Example/AppData/Local/Clearra/build'));
  assert.throws(() => assertBuildPathWithin('/tmp/clearra-build-other', '/tmp/clearra-build'));
  assert.throws(() => assertBuildPathWithin('/tmp/clearra-build/../escape', '/tmp/clearra-build'));
});

test('outside target and compiler overrides fail before directory creation', async t => {
  const options = await fixture(t);
  for (const key of ['CARGO_TARGET_DIR', 'CARGO_BUILD_TARGET_DIR', 'CARGO_BUILD_BUILD_DIR', 'CLEARRA_WSL_CARGO_TARGET_DIR', 'CLEARRA_CORE_C_BUILD_DIR', 'RUSTC_WRAPPER']) {
    await assert.rejects(acquireBuildOwner({ ...options, environment: { ...options.environment, [key]: join(options.temporary, 'outside') } }), /override|wrapper/iu);
  }
  await assert.rejects(stat(options.environment.LOCALAPPDATA), { code: 'ENOENT' });
});

test('experimental source purpose retains exactly one whole current build', async t => {
  const options = await fixture(t);
  const first = await acquireBuildOwner(options);
  const target = first.transaction.cargo_target_dir;
  await writeFile(join(target, 'old-hashed-dependency.rlib'), 'old');
  assert.equal(assertManagedBuildTransaction({ environment: first.environment, sourceRoot: options.sourceRoot }).cargoTarget, target);
  await first.finish(true);
  const second = await acquireBuildOwner(options);
  assert.equal(second.transaction.transaction_root, first.transaction.transaction_root);
  assert.notEqual(second.transaction.session_id, first.transaction.session_id);
  await assert.rejects(stat(join(target, 'old-hashed-dependency.rlib')), { code: 'ENOENT' });
  assert.deepEqual(await readdir(resolve(target, '../..')), ['current']);
  await second.finish(false);
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
  for (let index = 0; index < 7; index += 1) {
    const owner = await acquireBuildOwner({ ...options, purpose: 'product' });
    productRoot = resolve(owner.transaction.transaction_root, '..');
    await writeFile(join(owner.transaction.cargo_target_dir, 'product'), String(index));
    await owner.finish(true);
  }
  assert.equal((await readdir(productRoot)).length, 5);
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
  const childEnv = { ...process.env, ...owner.environment, CLEARRA_TEST_HELPER: helper, CLEARRA_TEST_PATHS: paths };
  const nested = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-Command',
    "$ErrorActionPreference='Stop'; . $env:CLEARRA_TEST_HELPER; Ensure-ClearraBuildArtifactCache -RepositoryRoot $env:CLEARRA_BUILD_SOURCE_ROOT; if(Test-ClearraBuildTransactionOwner){throw 'nested became owner'}; Get-ClearraCargoTargetDir; Exit-ClearraBuildArtifactCacheUsage"], { env: childEnv, encoding: 'utf8', windowsHide: true });
  assert.equal(nested.status, 0, nested.stderr + nested.stdout);
  assert.equal(buildPathIdentity(nested.stdout.trim()), buildPathIdentity(owner.transaction.cargo_target_dir));
  await owner.finish(true);
  const environment = { ...process.env, ...options.environment, CLEARRA_TEST_SOURCE: options.sourceRoot, CLEARRA_TEST_HELPER: helper, CLEARRA_TEST_PATHS: paths };
  const reverse = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-Command',
    "$ErrorActionPreference='Stop'; . $env:CLEARRA_TEST_HELPER; try { Ensure-ClearraBuildArtifactCache -RepositoryRoot $env:CLEARRA_TEST_SOURCE; node $env:CLEARRA_TEST_PATHS --field cargo-target; if($LASTEXITCODE -ne 0){throw 'Node rejected PS owner'}; Complete-ClearraBuildTransaction } finally { Exit-ClearraBuildArtifactCacheUsage }"], { env: environment, encoding: 'utf8', windowsHide: true });
  assert.equal(reverse.status, 0, reverse.stderr + reverse.stdout);
  assert.equal(buildPathIdentity(reverse.stdout.trim()), buildPathIdentity(owner.transaction.cargo_target_dir));
  const product = await acquireBuildOwner({ ...options, purpose: 'product' });
  const productConsumer = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-Command',
    "$ErrorActionPreference='Stop'; . $env:CLEARRA_TEST_HELPER; Ensure-ClearraBuildArtifactCache -RepositoryRoot $env:CLEARRA_BUILD_SOURCE_ROOT; if(Test-ClearraBuildTransactionOwner){throw 'nested product became owner'}; Get-ClearraCargoTargetDir; Exit-ClearraBuildArtifactCacheUsage"], {
    env: { ...process.env, ...product.environment, CLEARRA_TEST_HELPER: helper }, encoding: 'utf8', windowsHide: true,
  });
  assert.equal(productConsumer.status, 0, productConsumer.stderr + productConsumer.stdout);
  await product.finish(true);
});

test('bare Cargo fails before compiling or creating a target directory', async t => {
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
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /clearra-build-root-required/u);
  await assert.rejects(stat(join(options.sourceRoot, 'target')), { code: 'ENOENT' });
});
