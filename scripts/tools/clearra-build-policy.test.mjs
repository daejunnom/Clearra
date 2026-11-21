import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, mkdir, readFile, readdir, rm, stat, writeFile, symlink } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { basename, dirname, join, resolve, win32 } from 'node:path';
import { spawnSync } from 'node:child_process';
import { realpathSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { acquireBuildOwner, buildCompilerSnapshot, retainProductBuildGenerations } from './clearra-build-owner.mjs';
import { acquireManagedTransientDirectory } from './managed-transient-directory.mjs';
import { assertDefaultBenchmarkRustEnvironment } from './benchmark-rust-environment.mjs';
import { BUILD_TRANSACTION_MARKER, assertBuildPathWithin, assertCargoOutputArguments, assertManagedBuildTransaction, buildPathIdentity, buildSourceId, canonicalBuildRoot, canonicalNativeBuildPath, nativeBuildPath } from './clearra-build-policy.mjs';

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

async function retentionFixture(options, { purpose, index, snapshot, status = 'complete' }) {
  const root = canonicalBuildRoot(options.environment);
  const sourceRoot = canonicalNativeBuildPath(options.sourceRoot);
  const sourceId = buildSourceId(sourceRoot);
  const sessionId = (index + 1).toString(16).padStart(32, '0');
  const time = new Date(Date.UTC(2026, 10, 20, 0, 0, index)).toISOString();
  const transactionRoot = purpose === 'experiment'
    ? join(root, 'experiments', sourceId, 'current')
    : join(root, 'products', `${time.replace(/[-:.]/gu, '')}-${sessionId}`);
  const target = join(transactionRoot, 'cargo-target');
  await mkdir(target, { recursive: true });
  await mkdir(join(root, '.leases'), { recursive: true });
  const marker = {
    schema_version: snapshot ? 4 : 3, purpose, source_root: sourceRoot, source_id: sourceId,
    session_id: sessionId, transaction_root: transactionRoot, cargo_target_dir: target,
    owner_pid: process.pid, status, created_utc: time, completed_utc: status === 'complete' ? time : null,
  };
  if (snapshot) Object.assign(marker, {
    compiler_snapshot_sha256: snapshot.sourceSha256, compiler_input_file_count: snapshot.inputFileCount,
    incremental_cache_mode: 'enabled', incremental_context_sha256: snapshot.contextSha256,
    incremental_seed_session_id: null, incremental_seed_snapshot_sha256: null,
  });
  await writeFile(join(transactionRoot, BUILD_TRANSACTION_MARKER), JSON.stringify(marker));
  await writeFile(join(target, 'Cargo.lock'), 'fixture compiler input');
  return { root, transactionRoot, target, marker };
}

test('path identity is stable across Windows, WSL mounts, and WSL-native sources', () => {
  assert.equal(buildPathIdentity('C:\\Users\\Example\\AppData\\Local\\Clearra\\build'), buildPathIdentity('/mnt/c/Users/Example/AppData/Local/Clearra/build'));
  assert.equal(nativeBuildPath('/mnt/c/Users/Example/AppData/Local/Clearra/build', 'win32'), 'C:\\Users\\Example\\AppData\\Local\\Clearra\\build');
  assert.equal(nativeBuildPath('C:\\Users\\Example\\AppData\\Local\\Clearra\\build', 'linux'), '/mnt/c/Users/Example/AppData/Local/Clearra/build');
  assert.equal(buildPathIdentity('/home/clearra/workspaces/source'), '/home/clearra/workspaces/source');
  assert.throws(() => assertBuildPathWithin('/tmp/clearra-build-other', '/tmp/clearra-build'));
  assert.throws(() => assertBuildPathWithin('/tmp/clearra-build/../escape', '/tmp/clearra-build'));
});

test('GitHub Windows builds keep the canonical root on the checkout volume', async t => {
  // Only a native Windows host resolves existing prefixes on disk. Use its
  // own temporary fixture's drive rather than assume the CI runner has D:.
  const base = process.platform === 'win32' ? realpathSync.native((await fixture(t)).temporary) : 'D:\\a';
  const otherDrive = win32.parse(base).root.toUpperCase() === 'W:\\' ? 'Z:\\' : 'W:\\';
  const hosted = { GITHUB_ACTIONS: 'true', GITHUB_WORKSPACE: win32.join(base, 'Clearra', 'Clearra'),
    RUNNER_TEMP: win32.join(base, '_temp'), LOCALAPPDATA: win32.join(otherDrive, 'ignored-local-appdata') };
  assert.equal(canonicalBuildRoot(hosted, 'win32'), win32.join(hosted.RUNNER_TEMP, 'Clearra', 'build'));
  assert.throws(() => canonicalBuildRoot({ ...hosted, RUNNER_TEMP: win32.join(otherDrive, '_temp') }, 'win32'),
    /share the checkout volume/u);
  const localAppData = win32.join(base, 'local-appdata');
  assert.equal(canonicalBuildRoot({ LOCALAPPDATA: localAppData }, 'win32'), win32.join(localAppData, 'Clearra', 'build'));
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

test('Windows owner canonicalizes existing prefixes without admitting junctions', { skip: process.platform !== 'win32' }, async t => {
  const options = await fixture(t);
  const nativeTemporary = realpathSync.native(options.temporary);
  assert.equal(canonicalNativeBuildPath(options.sourceRoot), join(nativeTemporary, 'source'));
  const expectedRoot = join(nativeTemporary, 'cache', 'Clearra', 'build');
  assert.equal(canonicalBuildRoot(options.environment), expectedRoot);
  await assert.rejects(stat(options.environment.LOCALAPPDATA), { code: 'ENOENT' });
  const link = join(options.temporary, 'linked-source');
  await symlink(options.sourceRoot, link, 'junction');
  assert.throws(() => canonicalNativeBuildPath(join(link, 'not-created')), /symlink|junction/u);
  const owner = await acquireBuildOwner(options);
  try {
    assert.equal(owner.transaction.source_root, join(nativeTemporary, 'source'));
    assert.equal(owner.environment.CLEARRA_BUILD_ROOT, expectedRoot);
    assert.doesNotThrow(() => assertManagedBuildTransaction({ environment: owner.environment, sourceRoot: options.sourceRoot }));
  } finally { await owner.finish(false); }
});

test('outside target and compiler overrides fail before directory creation', async t => {
  const options = await fixture(t);
  for (const key of ['CARGO_TARGET_DIR', 'CARGO_BUILD_TARGET_DIR', 'CARGO_BUILD_BUILD_DIR', 'CLEARRA_WSL_CARGO_TARGET_DIR', 'CLEARRA_CORE_C_BUILD_DIR', 'RUSTC_WRAPPER']) {
    await assert.rejects(acquireBuildOwner({ ...options, environment: { ...options.environment, [key]: join(options.temporary, 'outside') } }), /override|wrapper/iu);
  }
  await assert.rejects(stat(options.environment.LOCALAPPDATA), { code: 'ENOENT' });
});

test('benchmark accepts only its active managed Rust guard and rejects performance overrides', async t => {
  const options = await fixture(t);
  const owner = await acquireBuildOwner(options);
  const keys = ['RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'RUSTFLAGS'];
  try {
    assert.doesNotThrow(() => assertDefaultBenchmarkRustEnvironment(keys, options.sourceRoot, owner.environment));
    assert.throws(() => assertDefaultBenchmarkRustEnvironment(keys, options.sourceRoot,
      { ...owner.environment, RUSTC_WRAPPER: join(options.temporary, 'other-wrapper') }), /unset RUSTC_WRAPPER/u);
    assert.throws(() => assertDefaultBenchmarkRustEnvironment(keys, options.sourceRoot,
      { ...owner.environment, RUSTFLAGS: '-C target-cpu=native' }), /unset RUSTFLAGS/u);
    assert.throws(() => assertDefaultBenchmarkRustEnvironment(keys, options.sourceRoot,
      { ...owner.environment, RUSTC_WORKSPACE_WRAPPER: 'another-wrapper' }), /RUSTC_WORKSPACE_WRAPPER/u);
  } finally { await owner.finish(false); }
  assert.throws(() => assertDefaultBenchmarkRustEnvironment(keys, options.sourceRoot, owner.environment));
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

test('an outer owner cannot complete or retire an unresolved inner staging slot', async t => {
  const options = await fixture(t);
  const owner = await acquireBuildOwner(options);
  const staging = await acquireManagedTransientDirectory(join(owner.transaction.transaction_root, 'wasm-stage'));
  const retainedFile = join(staging.path, 'keep.txt');
  await writeFile(retainedFile, 'still owned');
  await assert.rejects(owner.finish(true), /unresolved build scratch lease/u);
  await assert.rejects(acquireBuildOwner(options), /unresolved build scratch lease/u);
  assert.equal(await readFile(retainedFile, 'utf8'), 'still owned');
  await staging.release();
  const recovered = await acquireBuildOwner(options);
  assert.equal(recovered.transaction.incremental_seed_session_id, null);
  await recovered.finish(false);
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
  assert.throws(() => assertManagedBuildTransaction({
    environment: { ...owner.environment, CARGO_BUILD_TARGET_DIR: owner.environment.CARGO_TARGET_DIR },
  }), /override/u);
  assert.throws(() => assertManagedBuildTransaction({ environment: { ...owner.environment, CLEARRA_BUILD_SOURCE_ID: '0'.repeat(24) } }), /binding/u);
  assert.throws(() => assertCargoOutputArguments(['--out-dir', options.temporary], owner.transaction.transaction_root), /outside/u);
  assert.throws(() => assertCargoOutputArguments(['--emit=metadata=' + join(options.temporary, 'escape.rmeta')], owner.transaction.transaction_root), /outside/u);
  assert.throws(() => assertCargoOutputArguments(['--emit', 'metadata=' + join(options.temporary, 'escape.rmeta')], owner.transaction.transaction_root), /outside/u);
  assert.throws(() => assertCargoOutputArguments(['-o' + join(options.temporary, 'escape.rmeta')], owner.transaction.transaction_root), /outside/u);
  assert.doesNotThrow(() => assertCargoOutputArguments(['--out-dir', owner.transaction.cargo_target_dir, '--emit=dep-info,metadata'], owner.transaction.transaction_root));
  await owner.finish(false);
});

test('product rejects a fifth completed generation without deleting prior outputs', async t => {
  const options = await fixture(t);
  let productRoot;
  let crossHostGeneration;
  const failed = await acquireBuildOwner({ ...options, purpose: 'product' });
  await assert.rejects(acquireBuildOwner({ ...options, purpose: 'product' }), /independent product builds/u);
  await failed.finish(false);
  await assert.rejects(stat(failed.transaction.transaction_root), { code: 'ENOENT' });
  for (let index = 0; index < 4; index += 1) {
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
  const prior = await readdir(productRoot);
  assert.equal(prior.length, 4);
  if (crossHostGeneration) assert.ok((await stat(crossHostGeneration)).isDirectory());
  await assert.rejects(acquireBuildOwner({ ...options, purpose: 'product' }), {
    code: 'E_CLEARRA_BUILD_STORAGE_LIMIT',
  });
  await assert.rejects(acquireBuildOwner(options), { code: 'E_CLEARRA_BUILD_STORAGE_LIMIT' });
  assert.deepEqual(await readdir(productRoot), prior);
});

test('product retirement preflights and refuses to delete generations without proven handoff', async t => {
  const options = await fixture(t);
  const generations = [];
  for (let index = 0; index < 7; index += 1) {
    generations.push(await retentionFixture(options, { purpose: 'product', index }));
  }
  const [oldest, sibling] = generations;
  const foreign = join(options.temporary, 'foreign');
  await mkdir(foreign);
  await writeFile(join(foreign, 'preserve'), 'outside generation');
  const nested = join(oldest.target, 'nested');
  await mkdir(nested);
  const link = join(nested, 'link');
  await symlink(foreign, link, process.platform === 'win32' ? 'junction' : 'dir');
  await assert.rejects(retainProductBuildGenerations(oldest.root), {
    code: 'E_CLEARRA_BUILD_STORAGE_UNOWNED',
  });
  assert.equal(await readFile(join(sibling.target, 'Cargo.lock'), 'utf8'), 'fixture compiler input');
  assert.equal(await readFile(join(link, 'preserve'), 'utf8'), 'outside generation');
  assert.deepEqual(await readdir(join(oldest.root, '.leases')), []);

  // Removing only the injected link still does not prove that the pending
  // artifact consumers handed off the completed generations.
  await rm(link);
  const lease = join(oldest.root, '.leases', `product-${sibling.marker.session_id}.lock`);
  const otherOwner = { schema_version: 1, purpose: 'product', source_id: sibling.marker.source_id,
    session_id: sibling.marker.session_id, owner_pid: process.pid };
  await writeFile(lease, JSON.stringify(otherOwner));
  await assert.rejects(retainProductBuildGenerations(oldest.root), {
    code: 'E_CLEARRA_BUILD_STORAGE_LIMIT',
  });
  assert.equal(await readFile(join(oldest.target, 'Cargo.lock'), 'utf8'), 'fixture compiler input');
  assert.equal(await readFile(join(sibling.target, 'Cargo.lock'), 'utf8'), 'fixture compiler input');
  assert.deepEqual(JSON.parse(await readFile(lease, 'utf8')), otherOwner);
  assert.equal(await readFile(join(foreign, 'preserve'), 'utf8'), 'outside generation');
});

test('experiment cache retirement preserves every scratch sibling on foreign nested ownership', async t => {
  const options = await fixture(t);
  const snapshot = await buildCompilerSnapshot(options.sourceRoot, options.environment);
  const previous = await retentionFixture(options, { purpose: 'experiment', index: 0, snapshot });
  const early = join(previous.transactionRoot, 'a-scratch');
  const nested = join(previous.transactionRoot, 'z-scratch', 'nested');
  await mkdir(early);
  await mkdir(nested, { recursive: true });
  await writeFile(join(early, 'still-owned'), 'preserve early scratch');
  const previousMarker = join(previous.transactionRoot, BUILD_TRANSACTION_MARKER);
  const nestedMarker = join(nested, BUILD_TRANSACTION_MARKER);
  const previousContents = await readFile(previousMarker);
  const nestedContents = Buffer.from(JSON.stringify({ status: 'active' }));
  await writeFile(nestedMarker, nestedContents);
  await assert.rejects(acquireBuildOwner(options), { code: 'E_CLEARRA_BUILD_STORAGE_UNOWNED' });
  assert.equal(await readFile(join(early, 'still-owned'), 'utf8'), 'preserve early scratch');
  assert.deepEqual(await readFile(previousMarker), previousContents);
  assert.deepEqual(await readFile(nestedMarker), nestedContents);
  assert.equal(await readFile(join(previous.target, 'Cargo.lock'), 'utf8'), 'fixture compiler input');
  assert.deepEqual(await readdir(join(previous.root, '.leases')), []);
});

test('unknown current directory and junctions are never replaced', async t => {
  const options = await fixture(t);
  const root = canonicalBuildRoot(options.environment);
  const sourceId = buildSourceId(canonicalNativeBuildPath(options.sourceRoot));
  const current = join(root, 'experiments', sourceId, 'current');
  const userFile = join(current, 'user-file');
  await mkdir(current, { recursive: true });
  await writeFile(userFile, 'preserve');
  await assert.rejects(acquireBuildOwner(options), { code: 'E_CLEARRA_BUILD_STORAGE_INCOMPLETE' });
  assert.equal(await readFile(userFile, 'utf8'), 'preserve');
  await assert.rejects(stat(join(current, BUILD_TRANSACTION_MARKER)), { code: 'ENOENT' });
  assert.deepEqual(await readdir(join(root, '.leases')), []);
  const link = join(options.temporary, 'link');
  await symlink(options.sourceRoot, link, process.platform === 'win32' ? 'junction' : 'dir');
  assert.throws(() => assertCargoOutputArguments(['-o', join(link, 'escape')], options.temporary), /symlink|junction/u);
});

test('an active generation is preserved even after all leases are lost', async t => {
  const options = await fixture(t);
  const orphan = await retentionFixture(options, { purpose: 'experiment', index: 0, status: 'active' });
  const marker = join(orphan.transactionRoot, BUILD_TRANSACTION_MARKER);
  const original = await readFile(marker);
  assert.deepEqual(await readdir(join(orphan.root, '.leases')), []);
  await assert.rejects(acquireBuildOwner(options), { code: 'E_CLEARRA_BUILD_STORAGE_UNOWNED' });
  assert.deepEqual(await readFile(marker), original);
  assert.equal(await readFile(join(orphan.target, 'Cargo.lock'), 'utf8'), 'fixture compiler input');
  assert.deepEqual(await readdir(join(orphan.root, '.leases')), []);
});

test('Node and PowerShell reuse the same lease without nested completion', { skip: process.platform !== 'win32' }, async t => {
  const options = await fixture(t);
  const owner = await acquireBuildOwner(options);
  const helper = fileURLToPath(new URL('../lib/clearra-path-helpers.ps1', import.meta.url));
  const paths = fileURLToPath(new URL('./clearra-build-paths.mjs', import.meta.url));
  const childEnv = { ...cleanBuildEnvironment(), ...owner.environment, CLEARRA_TEST_HELPER: helper, CLEARRA_TEST_PATHS: paths };
  const probe = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-Command',
    "$ErrorActionPreference='Stop'; . $env:CLEARRA_TEST_HELPER; [pscustomobject]@{actual=[IO.Path]::GetFullPath($env:RUSTC_WRAPPER);expected=(Get-ClearraExpectedRustcWrapper);source=[IO.Path]::GetFullPath($env:CLEARRA_BUILD_SOURCE_ROOT);cargo=[IO.Path]::GetFullPath($env:CARGO_TARGET_DIR);root=(Get-ClearraCanonicalBuildRoot);transaction=$env:CLEARRA_BUILD_TRANSACTION_ROOT;windows=(Test-StartTestsWindows)} | ConvertTo-Json -Compress"],
    { env: childEnv, encoding: 'utf8', windowsHide: true });
  assert.equal(probe.status, 0, probe.stderr + probe.stdout);
  const guardProjection = JSON.parse(probe.stdout);
  assert.equal(guardProjection.windows, true, JSON.stringify(guardProjection));
  assert.equal(buildPathIdentity(guardProjection.transaction), buildPathIdentity(owner.transaction.transaction_root), JSON.stringify(guardProjection));
  assert.equal(buildPathIdentity(guardProjection.actual), buildPathIdentity(guardProjection.expected), JSON.stringify(guardProjection));
  assert.equal(buildPathIdentity(guardProjection.source), buildPathIdentity(owner.transaction.source_root), JSON.stringify(guardProjection));
  assert.equal(buildPathIdentity(guardProjection.cargo), buildPathIdentity(owner.transaction.cargo_target_dir), JSON.stringify(guardProjection));
  assert.equal(buildPathIdentity(guardProjection.root), buildPathIdentity(owner.environment.CLEARRA_BUILD_ROOT), JSON.stringify(guardProjection));
  for (const wrapper of [owner.environment.RUSTC_WRAPPER, owner.environment.RUSTC_WRAPPER.replaceAll('\\', '/')]) {
    const nested = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-Command',
      "$ErrorActionPreference='Stop'; . $env:CLEARRA_TEST_HELPER; Ensure-ClearraBuildArtifactCache -RepositoryRoot $env:CLEARRA_BUILD_SOURCE_ROOT; if(Test-ClearraBuildTransactionOwner){throw 'nested became owner'}; Get-ClearraCargoTargetDir; Exit-ClearraBuildArtifactCacheUsage"],
      { env: { ...childEnv, RUSTC_WRAPPER: wrapper }, encoding: 'utf8', windowsHide: true });
    assert.equal(nested.status, 0, JSON.stringify(guardProjection) + '\n' + nested.stderr + nested.stdout);
    assert.equal(buildPathIdentity(nested.stdout.trim()), buildPathIdentity(owner.transaction.cargo_target_dir));
  }
  const foreign = spawnSync('pwsh', ['-NoProfile', '-NonInteractive', '-Command',
    "$ErrorActionPreference='Stop'; . $env:CLEARRA_TEST_HELPER; Ensure-ClearraBuildArtifactCache -RepositoryRoot $env:CLEARRA_BUILD_SOURCE_ROOT"],
    { env: { ...childEnv, RUSTC_WRAPPER: join(options.temporary, 'other-wrapper.exe') }, encoding: 'utf8', windowsHide: true });
  assert.notEqual(foreign.status, 0);
  assert.match(foreign.stderr, /Unmanaged RUSTC_WRAPPER/u);
  assert.equal(assertManagedBuildTransaction({ environment: owner.environment }).session_id, owner.transaction.session_id);
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
