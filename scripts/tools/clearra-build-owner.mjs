// Native Node owner for hosts without PowerShell (notably the Bookworm builder).
// Lease and marker schemas are shared with clearra-build-transaction.ps1.
import { createHash, randomBytes } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { lstat, mkdir, open, opendir, readFile, readdir, rename, rm, stat, writeFile } from 'node:fs/promises';
import { basename, dirname, extname, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { prepareRustcLauncher } from './prepare-clearra-rustc-launcher.mjs';
import { assertManagedTransientSlotsRetired } from './managed-transient-directory.mjs';
import { BUILD_STORAGE_POLICY, checkBuildStorage, monitorBuildStorage } from './clearra-build-capacity.mjs';
import { BUILD_STORAGE_LEASE, BUILD_TRANSACTION_MARKER, BUILD_TRANSACTION_SCHEMA, FORBIDDEN_BUILD_ALIASES, assertBuildRecord, assertBuildPathWithin, assertNoBuildLinks, assertManagedBuildTransaction,
  buildPathIdentity, buildSourceId, canonicalBuildRoot, canonicalNativeBuildPath, nativeBuildPath } from './clearra-build-policy.mjs';

const authorityRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const buildInputRoots = ['apps', 'assets', 'core-c', 'crates', 'packages', 'scripts', 'tests', 'tools'];
const buildInputFiles = ['Cargo.toml', 'Cargo.lock', 'CMakeLists.txt', 'package.json', 'rust-toolchain.toml',
  '.cargo/config.toml', 'config/build-storage.v1.json'];
const generatedDirectories = new Set(['.git', '.cache', '.svelte-kit', '.vite-temp', '_local', 'dist', 'dist-server',
  'node_modules', 'build', 'models', 'checkpoints']);
const retirementEntryLimit = BUILD_STORAGE_POLICY.maxEntries;
const retirementDepthLimit = BUILD_STORAGE_POLICY.maxDepth;

function secretOrGeneratedInput(path) {
  const name = basename(path);
  return name === 'pnpm-lock.yaml' || name === '.env' || name.toLowerCase().startsWith('.env.') ||
    /(credential|service[-_]?account|api[-_]?key|^id_(rsa|dsa|ecdsa|ed25519)(\.|$)|^authorized_keys$)/iu.test(name) ||
    /^\.(pem|key|pfx|p12)$/iu.test(extname(name));
}
function generatedInputDirectory(path) {
  const name = basename(path);
  if (generatedDirectories.has(name)) return true;
  if (name === 'schemas' && basename(dirname(path)) === 'gen' && basename(dirname(dirname(path))) === 'src-tauri') return true;
  if (name === 'wasm' && basename(dirname(path)) === 'static' && basename(dirname(dirname(path))) === 'clearra-web' && basename(dirname(dirname(dirname(path)))) === 'apps') return true;
  return ['target', 'coverage'].includes(name) && !['src', 'fixtures', 'golden'].includes(basename(dirname(path)));
}
async function collectBuildInputFiles(sourceRoot) {
  const inputs = [];
  const addFile = async path => {
    let metadata;
    try { metadata = await lstat(path); }
    catch (error) { if (error.code === 'ENOENT') return; throw error; }
    if (metadata.isSymbolicLink()) throw new Error(`Clearra compiler snapshot refuses a linked input: ${path}`);
    if (metadata.isFile() && !secretOrGeneratedInput(path)) inputs.push(path);
  };
  for (const name of buildInputFiles) await addFile(resolve(sourceRoot, name));
  for (const relativeRoot of buildInputRoots) {
    const root = resolve(sourceRoot, relativeRoot);
    let rootMetadata;
    try { rootMetadata = await lstat(root); }
    catch (error) { if (error.code === 'ENOENT') continue; throw error; }
    if (rootMetadata.isSymbolicLink()) throw new Error(`Clearra compiler snapshot refuses a linked input root: ${root}`);
    if (!rootMetadata.isDirectory()) continue;
    const pending = [root];
    while (pending.length > 0) {
      const directory = pending.pop();
      const entries = await readdir(directory, { withFileTypes: true });
      for (const entry of entries) {
        const path = resolve(directory, entry.name);
        if (entry.isSymbolicLink()) throw new Error(`Clearra compiler snapshot refuses a linked input: ${path}`);
        if (entry.isDirectory()) {
          if (!generatedInputDirectory(path)) pending.push(path);
        } else if (entry.isFile() && !secretOrGeneratedInput(path)) inputs.push(path);
      }
    }
  }
  return [...new Set(inputs)].sort((left, right) => left.localeCompare(right, 'en', { sensitivity: 'case' }));
}
function commandVersion(command, arguments_, environment) {
  const result = spawnSync(command, arguments_, {
    encoding: 'utf8', env: { ...process.env, ...environment }, windowsHide: true, timeout: 15000, maxBuffer: 1024 * 1024,
  });
  if (result.error || result.signal || result.status !== 0) return `${command}=unavailable`;
  // rustup's first-use installation writes transient diagnostics to stderr.
  // Successful stdout is the stable compiler identity for the build snapshot.
  return `${command}=${`${result.stdout ?? ''}`.trim().replace(/\r?\n/gu, '|')}`;
}
export async function buildCompilerSnapshot(sourceRoot, environment = process.env) {
  const files = await collectBuildInputFiles(sourceRoot);
  const sourceHash = createHash('sha256');
  sourceHash.update('clearra.compiler-input-snapshot.v1\n');
  for (const path of files) {
    const bytes = await readFile(path);
    const relativePath = relative(sourceRoot, path).replaceAll('\\', '/');
    const digest = createHash('sha256').update(bytes).digest('hex');
    sourceHash.update(`${relativePath}\0${bytes.byteLength}\0${digest}\n`);
  }
  const context = [
    'schema=clearra.incremental-context.v1',
    'owner=node-v1',
    `platform=${process.platform}`,
    `architecture=${process.arch}`,
    `execution_surface=${environment.CLEARRA_EXECUTION_SURFACE ?? ''}`,
    `rustflags=${environment.RUSTFLAGS ?? ''}`,
    `windows_rustflags=${environment.CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS ?? ''}`,
    `cargo_build_target=${environment.CARGO_BUILD_TARGET ?? ''}`,
    commandVersion('cargo', ['--version', '--verbose'], environment),
    commandVersion('rustc', ['--version', '--verbose'], environment),
    commandVersion('cmake', ['--version'], environment),
  ];
  return {
    sourceSha256: sourceHash.digest('hex'),
    inputFileCount: files.length,
    contextSha256: createHash('sha256').update(context.join('\n')).digest('hex'),
  };
}
function leaseIdentity(marker, ownerPid = marker.owner_pid) {
  return { schema_version: 1, purpose: marker.purpose, source_id: marker.source_id, session_id: marker.session_id, owner_pid: ownerPid };
}
function leasePath(root, marker) {
  return resolve(root, '.leases', marker.purpose === 'experiment' ? `experiment-${marker.source_id}.lock` : `product-${marker.session_id}.lock`);
}
async function readJson(path) {
  assertNoBuildLinks(path);
  const size = await stat(path);
  if (!size.isFile() || size.size > 16384) throw new Error('Invalid Clearra build ownership metadata');
  return JSON.parse((await readFile(path, 'utf8')).replace(/^\uFEFF/u, ''));
}
async function writeMarker(marker) {
  const path = resolve(marker.transaction_root, BUILD_TRANSACTION_MARKER);
  const temporary = `${path}.${marker.session_id}.next`;
  await writeFile(temporary, `${JSON.stringify(marker)}\n`, { flag: 'wx' });
  await rename(temporary, path);
}
async function claimLease(path, identity) {
  const handle = await open(path, 'wx');
  try { await handle.writeFile(`${JSON.stringify(identity)}\n`); }
  finally { await handle.close(); }
}
async function releaseLease(path, identity) {
  const current = await readJson(path);
  if (Object.keys(identity).some(key => current[key] !== identity[key])) throw new Error('Refusing to release another build owner lease');
  await rm(path);
}
async function validateRetirement(marker, root, identity) {
  const transactionRoot = nativeBuildPath(marker.transaction_root);
  assertBuildRecord(marker, root, transactionRoot);
  assertBuildPathWithin(transactionRoot, root);
  assertNoBuildLinks(transactionRoot);
  const current = await readJson(resolve(transactionRoot, BUILD_TRANSACTION_MARKER));
  assertBuildRecord(current, root, transactionRoot);
  if (Object.keys(marker).some(key => current[key] !== marker[key]) || current.status === 'active') {
    throw new Error('Build generation ownership changed before retirement');
  }
  if (buildPathIdentity(marker.transaction_root) === buildPathIdentity(root)) throw new Error('Cannot remove the whole build root');
  await assertManagedTransientSlotsRetired(transactionRoot);
  const lease = await readJson(leasePath(root, marker));
  if (identity.owner_pid !== process.pid || identity.purpose !== marker.purpose || identity.source_id !== marker.source_id ||
      (marker.purpose === 'product' && identity.session_id !== marker.session_id) ||
      Object.keys(identity).some(key => lease[key] !== identity[key])) {
    throw new Error('Build cleanup requires the current source/purpose owner lease');
  }
  return transactionRoot;
}
async function inspectRetirementTree(path, transactionRoot, inventory, depth = 0) {
  if (++inventory.entries > retirementEntryLimit || depth > retirementDepthLimit) {
    throw new Error(`Managed build cleanup inventory exceeds its bound: ${transactionRoot}`);
  }
  const stat = await lstat(path);
  if (stat.isSymbolicLink()) throw new Error(`Managed build cleanup refuses a nested symlink or junction: ${path}`);
  const name = basename(path);
  const parent = dirname(path);
  const directlyOwnedScratch = parent === transactionRoot || parent === resolve(transactionRoot, 'transient');
  // The root marker and defined scratch sidecars have already been checked.
  // Do not mistake compiler lockfiles for Clearra ownership metadata.
  if ((name === BUILD_TRANSACTION_MARKER && parent !== transactionRoot) || name === '.leases' ||
      ((name.endsWith('.owner.json') || name.endsWith('.owner.json.pending')) && !directlyOwnedScratch)) {
    throw new Error(`Managed build cleanup refuses foreign nested ownership metadata: ${path}`);
  }
  if (stat.isDirectory()) {
    const directory = await opendir(path);
    for await (const child of directory) {
      await inspectRetirementTree(resolve(path, child.name), transactionRoot, inventory, depth + 1);
    }
  } else if (!stat.isFile()) {
    throw new Error(`Managed build cleanup refuses an unknown filesystem entry: ${path}`);
  }
  return stat.isDirectory();
}
async function preflightOwnedGeneration(marker, root, identity, inventory) {
  const transactionRoot = await validateRetirement(marker, root, identity);
  await inspectRetirementTree(transactionRoot, transactionRoot, inventory);
}
async function removePreflightedGeneration(marker, root, identity) {
  const transactionRoot = await validateRetirement(marker, root, identity);
  await rm(transactionRoot, { recursive: true, force: false });
}
async function removeOwnedGeneration(marker, root, identity) {
  await preflightOwnedGeneration(marker, root, identity, { entries: 0 });
  await removePreflightedGeneration(marker, root, identity);
}
async function retainExperimentCompilerCache(marker, root, identity) {
  const transactionRoot = nativeBuildPath(marker.transaction_root);
  assertBuildRecord(marker, root, transactionRoot);
  if (marker.schema_version !== BUILD_TRANSACTION_SCHEMA || marker.purpose !== 'experiment' || marker.status !== 'complete') return false;
  const target = resolve(transactionRoot, 'cargo-target');
  let metadata;
  try { metadata = await lstat(target); }
  catch (error) { if (error.code === 'ENOENT') return false; throw error; }
  assertNoBuildLinks(target);
  if (!metadata.isDirectory() || metadata.isSymbolicLink()) return false;
  await validateRetirement(marker, root, identity);
  const scratch = [];
  const inventory = { entries: 0 };
  for (const entry of await readdir(transactionRoot, { withFileTypes: true })) {
    if ([BUILD_TRANSACTION_MARKER, 'cargo-target'].includes(entry.name)) continue;
    const path = resolve(transactionRoot, entry.name);
    const recursive = await inspectRetirementTree(path, transactionRoot, inventory);
    scratch.push({ path, recursive });
  }
  // Do not retire an early sibling before a later sibling has passed preflight.
  await validateRetirement(marker, root, identity);
  for (const { path, recursive } of scratch) {
    assertNoBuildLinks(path);
    await rm(path, { recursive, force: false });
  }
  return true;
}
export async function retainProductBuildGenerations(root) {
  // A completed product may still be read by a later artifact upload outside
  // this process. Until that handoff is explicitly proven, absence of a
  // compiler lease is not permission to evict it.
  return checkBuildStorage(root);
}
async function assertCleanProductCatalog(root) {
  const directory = resolve(root, 'products');
  assertNoBuildLinks(directory);
  const entries = await readdir(directory, { withFileTypes: true }).catch(error => { if (error.code === 'ENOENT') return []; throw error; });
  for (const entry of entries) {
    const path = resolve(directory, entry.name);
    if (!entry.isDirectory() || entry.isSymbolicLink()) throw new Error('Unowned product catalog entry; cleanup must be explicit');
    const record = await readJson(resolve(path, BUILD_TRANSACTION_MARKER));
    assertBuildRecord(record, root, path);
    if (record.purpose !== 'product' || record.status !== 'complete') throw new Error('An unfinished product build requires explicit cleanup before a new product build');
  }
  for (const entry of await readdir(resolve(root, '.leases'))) {
    if (/^product-[a-f0-9]{32}\.lock$/u.test(entry)) throw new Error('An active or stale product lease requires explicit cleanup');
  }
}
async function assertExperimentalSlotOwnership(root, transactionRoot) {
  let previous;
  try { previous = await readJson(resolve(transactionRoot, BUILD_TRANSACTION_MARKER)); }
  catch (error) { if (error.code !== 'ENOENT') throw error; }
  if (!previous) {
    try { await stat(transactionRoot); throw new Error('Experimental slot exists without ownership metadata'); }
    catch (error) { if (error.code !== 'ENOENT') throw error; }
    return;
  }
  assertBuildRecord(previous, root, transactionRoot);
  if (previous.status === 'active') {
    try { await readJson(leasePath(root, previous)); }
    catch (error) {
      if (error.code === 'ENOENT') throw new Error('An active experimental generation cannot be replaced, even without a lease');
      throw error;
    }
  }
}
export async function acquireBuildOwner({ sourceRoot, purpose = 'experiment', environment = process.env } = {}) {
  sourceRoot = canonicalNativeBuildPath(sourceRoot);
  if (!['experiment', 'product'].includes(purpose)) throw new Error('Unknown Clearra build purpose');
  if (environment.CLEARRA_BUILD_SESSION_ID) {
    const transaction = assertManagedBuildTransaction({ environment, sourceRoot });
    if (transaction.purpose !== purpose) throw new Error('Nested build purpose cannot change');
    return { environment, transaction, finish: async () => {}, monitor: () => async () => {} };
  }
  const root = canonicalBuildRoot(environment);
  const sourceId = buildSourceId(sourceRoot);
  const sessionId = randomBytes(16).toString('hex');
  const created = new Date().toISOString();
  const transactionRoot = purpose === 'experiment' ? resolve(root, 'experiments', sourceId, 'current') : resolve(root, 'products', `${created.replace(/[-:.]/gu, '')}-${sessionId}`);
  const cargoTarget = resolve(transactionRoot, 'cargo-target');
  const guard = process.platform === 'win32' ? resolve(transactionRoot, 'build-tools/clearra-rustc-guard.exe') : resolve(authorityRoot, 'scripts/tools/clearra-rustc-guard.sh');
  for (const key of ['CARGO_TARGET_DIR']) {
    if (environment[key] && (purpose !== 'experiment' || buildPathIdentity(environment[key]) !== buildPathIdentity(cargoTarget))) throw new Error(`External build target override is forbidden: ${key}`);
  }
  for (const key of FORBIDDEN_BUILD_ALIASES) {
    if (environment[key]) throw new Error(`Independent build layout/wrapper override is forbidden: ${key}`);
  }
  if (environment.CLEARRA_BUILD_ROOT && buildPathIdentity(environment.CLEARRA_BUILD_ROOT) !== buildPathIdentity(root)) throw new Error('Clearra build root override is forbidden');
  if (environment.RUSTC_WRAPPER && buildPathIdentity(environment.RUSTC_WRAPPER) !== buildPathIdentity(guard)) throw new Error('Unmanaged Rust compiler wrapper is forbidden');
  assertNoBuildLinks(root);
  assertNoBuildLinks(transactionRoot);
  assertNoBuildLinks(sourceRoot);
  await stat(resolve(sourceRoot, 'Cargo.toml'));
  // Reject known foreign or interrupted slots before catalog observation can
  // obscure their ownership error. This read grants no ownership: admission,
  // leases and the marker recheck below still precede every replacement.
  if (purpose === 'experiment') await assertExperimentalSlotOwnership(root, transactionRoot);
  const compilerSnapshot = await buildCompilerSnapshot(sourceRoot, environment);
  const marker = { schema_version: BUILD_TRANSACTION_SCHEMA, purpose, source_root: sourceRoot, source_id: sourceId, session_id: sessionId,
    transaction_root: transactionRoot, cargo_target_dir: cargoTarget, owner_pid: process.pid, status: 'active', created_utc: created, completed_utc: null,
    compiler_snapshot_sha256: compilerSnapshot.sourceSha256, compiler_input_file_count: compilerSnapshot.inputFileCount,
    incremental_cache_mode: purpose === 'experiment' ? 'enabled' : 'disabled', incremental_context_sha256: compilerSnapshot.contextSha256,
    incremental_seed_session_id: null, incremental_seed_snapshot_sha256: null };
  const identity = leaseIdentity(marker);
  assertNoBuildLinks(resolve(root, '.leases'));
  await mkdir(resolve(root, '.leases'), { recursive: true });
  const path = leasePath(root, marker);
  const capacityPath = resolve(root, '.leases', BUILD_STORAGE_LEASE);
  const catalogPath = resolve(root, '.leases/products-catalog.lock');
  let catalogOwned = false;
  let slotOwned = false;
  assertNoBuildLinks(capacityPath);
  try { await claimLease(capacityPath, identity); }
  catch (error) {
    if (error.code === 'EEXIST') throw new Error('Clearra build storage already has an owner or stale lease; independent product builds cannot overlap and no automatic takeover is allowed');
    throw error;
  }
  const releaseOwnedLeases = async () => {
    try {
      if (slotOwned) await releaseLease(path, identity);
    } finally {
      try {
        if (catalogOwned) await releaseLease(catalogPath, identity);
      } finally {
        await releaseLease(capacityPath, identity);
      }
    }
  };
  try {
    if (purpose === 'product') {
      assertNoBuildLinks(catalogPath);
      try { await claimLease(catalogPath, identity); catalogOwned = true; }
      catch (error) { if (error.code === 'EEXIST') throw new Error('Another product owner or stale catalog lease exists; independent product builds cannot overlap'); throw error; }
      await assertCleanProductCatalog(root);
    }
    await checkBuildStorage(root, { phase: 'admit', slot: transactionRoot });
    assertNoBuildLinks(path);
    try { await claimLease(path, identity); slotOwned = true; }
    catch (error) { if (error.code === 'EEXIST') throw new Error('Build purpose already has an owner or stale lease; no automatic takeover is allowed'); throw error; }
    if (purpose === 'experiment') {
      let previous;
      try { previous = await readJson(resolve(transactionRoot, BUILD_TRANSACTION_MARKER)); }
      catch (error) { if (error.code !== 'ENOENT') throw error; }
      if (previous) {
        assertBuildRecord(previous, root, transactionRoot);
        if (previous.status === 'active') throw new Error('An active experimental generation cannot be replaced, even without a lease');
        if (previous.source_id !== sourceId || previous.purpose !== purpose || buildPathIdentity(previous.transaction_root) !== buildPathIdentity(transactionRoot)) throw new Error('Experimental current slot has a different owner');
        const reusable = previous.schema_version === BUILD_TRANSACTION_SCHEMA && previous.status === 'complete' &&
          previous.incremental_cache_mode === 'enabled' && previous.incremental_context_sha256 === compilerSnapshot.contextSha256 &&
          await retainExperimentCompilerCache(previous, root, identity);
        if (reusable) {
          marker.incremental_seed_session_id = previous.session_id;
          marker.incremental_seed_snapshot_sha256 = previous.compiler_snapshot_sha256;
        } else {
          await removeOwnedGeneration(previous, root, identity);
        }
      } else {
        try { await stat(transactionRoot); throw new Error('Experimental slot exists without ownership metadata'); }
        catch (error) { if (error.code !== 'ENOENT') throw error; }
      }
    }
    await mkdir(cargoTarget, { recursive: true });
    await writeMarker(marker);
  } catch (error) {
    await releaseOwnedLeases();
    throw error;
  }
  const childEnvironment = { ...environment, CLEARRA_BUILD_ROOT: root, CLEARRA_BUILD_PURPOSE: purpose,
    CLEARRA_BUILD_SOURCE_ROOT: sourceRoot, CLEARRA_BUILD_SOURCE_ID: sourceId, CLEARRA_BUILD_SESSION_ID: sessionId,
    CLEARRA_BUILD_TRANSACTION_ROOT: transactionRoot, CLEARRA_BUILD_CACHE_OWNER_PID: String(process.pid),
    CLEARRA_BUILD_CACHE_SESSION_KEY: sessionId, CARGO_TARGET_DIR: cargoTarget,
    CARGO_INCREMENTAL: purpose === 'experiment' ? '1' : '0', RUSTC_WRAPPER: guard };
  if (process.platform !== 'win32' && /^[A-Za-z]:\//u.test(buildPathIdentity(root))) childEnvironment.LOCALAPPDATA = buildPathIdentity(root).replace(/\/Clearra\/build$/iu, '').replaceAll('/', '\\');
  let finished = false;
  const owner = {
    environment: childEnvironment,
    transaction: marker,
    monitor: onError => monitorBuildStorage(async () => {
      assertManagedBuildTransaction({ environment: childEnvironment, sourceRoot });
      await checkBuildStorage(root, { slot: transactionRoot });
      assertManagedBuildTransaction({ environment: childEnvironment, sourceRoot });
    }, onError),
    finish: async (success, { preserveOutputs = false } = {}) => {
    if (finished) return;
    assertManagedBuildTransaction({ environment: childEnvironment, sourceRoot });
    finished = true;
    let snapshotError;
    if (success) {
      try {
        await checkBuildStorage(root, { phase: 'complete', slot: transactionRoot });
        await assertManagedTransientSlotsRetired(transactionRoot);
        const finalSnapshot = await buildCompilerSnapshot(sourceRoot, environment);
        if (finalSnapshot.sourceSha256 !== marker.compiler_snapshot_sha256 ||
            finalSnapshot.inputFileCount !== marker.compiler_input_file_count ||
            finalSnapshot.contextSha256 !== marker.incremental_context_sha256) {
          snapshotError = new Error('Clearra build inputs changed while the transaction was active; the incremental seed was not sealed');
          success = false;
        }
      } catch (error) {
        snapshotError = error;
        success = false;
        if (error.code?.startsWith('E_CLEARRA_BUILD_STORAGE_')) preserveOutputs = true;
      }
    }
    marker.status = success ? 'complete' : 'failed';
    marker.completed_utc = success ? new Date().toISOString() : null;
    try {
      await writeMarker(marker);
      if (success) {
        try { await checkBuildStorage(root, { phase: 'complete', slot: transactionRoot }); }
        catch (error) {
          marker.status = 'failed';
          marker.completed_utc = null;
          await writeMarker(marker);
          throw error;
        }
      }
      if (purpose === 'product' && !success && !preserveOutputs) await removeOwnedGeneration(marker, root, identity);
    } finally { await releaseOwnedLeases(); }
    if (snapshotError) throw snapshotError;
  } };
  try {
    if (process.platform === 'win32') prepareRustcLauncher(childEnvironment);
    await checkBuildStorage(root, { slot: transactionRoot });
  } catch (error) {
    await owner.finish(false, { preserveOutputs: error.code?.startsWith('E_CLEARRA_BUILD_STORAGE_') ?? false });
    throw error;
  }
  return owner;
}
