// Native Node owner for hosts without PowerShell (notably the Bookworm builder).
// Lease and marker schemas are shared with clearra-build-transaction.ps1.
import { randomBytes } from 'node:crypto';
import { mkdir, open, readFile, readdir, rename, rm, stat, writeFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { BUILD_TRANSACTION_MARKER, FORBIDDEN_BUILD_ALIASES, assertBuildRecord, assertBuildPathWithin, assertNoBuildLinks, assertManagedBuildTransaction,
  buildPathIdentity, buildSourceId, canonicalBuildRoot } from './clearra-build-policy.mjs';

const authorityRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
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
async function removeOwnedGeneration(marker, root) {
  assertBuildRecord(marker, root, marker.transaction_root);
  assertBuildPathWithin(marker.transaction_root, root);
  assertNoBuildLinks(marker.transaction_root);
  const current = await readJson(resolve(marker.transaction_root, BUILD_TRANSACTION_MARKER));
  for (const key of ['schema_version', 'purpose', 'source_root', 'source_id', 'session_id', 'transaction_root', 'cargo_target_dir', 'owner_pid']) {
    if (current[key] !== marker[key]) throw new Error('Build generation ownership changed before retirement');
  }
  if (buildPathIdentity(marker.transaction_root) === buildPathIdentity(root)) throw new Error('Cannot remove the whole build root');
  await rm(marker.transaction_root, { recursive: true, force: false });
}
export async function retainProductBuildGenerations(root) {
  const productRoot = resolve(root, 'products');
  assertNoBuildLinks(productRoot);
  const entries = await readdir(productRoot, { withFileTypes: true }).catch(error => { if (error.code === 'ENOENT') return []; throw error; });
  const complete = [];
  for (const entry of entries) {
    if (!entry.isDirectory() || entry.isSymbolicLink()) continue;
    const directory = resolve(productRoot, entry.name);
    const marker = await readJson(resolve(directory, BUILD_TRANSACTION_MARKER));
    assertBuildRecord(marker, root, directory);
    if (marker.purpose !== 'product') throw new Error('Unowned product build directory; refusing retention');
    if (marker.status === 'complete' && Number.isFinite(Date.parse(marker.completed_utc))) complete.push(marker);
  }
  complete.sort((a, b) => b.completed_utc.localeCompare(a.completed_utc) || b.session_id.localeCompare(a.session_id));
  for (const marker of complete.slice(5)) {
    const path = leasePath(root, marker);
    const identity = leaseIdentity(marker, process.pid);
    try { await claimLease(path, identity); }
    catch (error) { if (error.code === 'EEXIST') continue; throw error; }
    try { await removeOwnedGeneration(marker, root); }
    finally { await releaseLease(path, identity); }
  }
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
export async function acquireBuildOwner({ sourceRoot, purpose = 'experiment', environment = process.env } = {}) {
  sourceRoot = resolve(sourceRoot);
  if (!['experiment', 'product'].includes(purpose)) throw new Error('Unknown Clearra build purpose');
  if (environment.CLEARRA_BUILD_SESSION_ID) {
    const transaction = assertManagedBuildTransaction({ environment, sourceRoot });
    if (transaction.purpose !== purpose) throw new Error('Nested build purpose cannot change');
    return { environment, transaction, finish: async () => {} };
  }
  const root = canonicalBuildRoot(environment);
  const sourceId = buildSourceId(sourceRoot);
  const sessionId = randomBytes(16).toString('hex');
  const created = new Date().toISOString();
  const transactionRoot = purpose === 'experiment' ? resolve(root, 'experiments', sourceId, 'current') : resolve(root, 'products', `${created.replace(/[-:.]/gu, '')}-${sessionId}`);
  const cargoTarget = resolve(transactionRoot, 'cargo-target');
  const guard = resolve(authorityRoot, 'scripts/tools', process.platform === 'win32' ? 'clearra-rustc-guard.cmd' : 'clearra-rustc-guard.sh');
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
  const marker = { schema_version: 3, purpose, source_root: sourceRoot, source_id: sourceId, session_id: sessionId,
    transaction_root: transactionRoot, cargo_target_dir: cargoTarget, owner_pid: process.pid, status: 'active', created_utc: created, completed_utc: null };
  const identity = leaseIdentity(marker);
  assertNoBuildLinks(resolve(root, '.leases'));
  await mkdir(resolve(root, '.leases'), { recursive: true });
  const path = leasePath(root, marker);
  const catalogPath = resolve(root, '.leases/products-catalog.lock');
  let catalogOwned = false;
  if (purpose === 'product') {
    try { await claimLease(catalogPath, identity); catalogOwned = true; }
    catch (error) { if (error.code === 'EEXIST') throw new Error('Another product owner or stale catalog lease exists; independent product builds cannot overlap'); throw error; }
    try { await assertCleanProductCatalog(root); await retainProductBuildGenerations(root); }
    catch (error) { await releaseLease(catalogPath, identity); throw error; }
  }
  try { await claimLease(path, identity); }
  catch (error) {
    if (catalogOwned) await releaseLease(catalogPath, identity);
    if (error.code === 'EEXIST') throw new Error('Build purpose already has an owner or stale lease; no automatic takeover is allowed');
    throw error;
  }
  try {
    if (purpose === 'experiment') {
      let previous;
      try { previous = await readJson(resolve(transactionRoot, BUILD_TRANSACTION_MARKER)); }
      catch (error) { if (error.code !== 'ENOENT') throw error; }
      if (previous) {
        assertBuildRecord(previous, root, transactionRoot);
        if (previous.status === 'active') throw new Error('An active experimental generation cannot be replaced, even without a lease');
        if (previous.schema_version !== 3 || previous.source_id !== sourceId || previous.purpose !== purpose || buildPathIdentity(previous.transaction_root) !== buildPathIdentity(transactionRoot)) throw new Error('Experimental current slot has a different owner');
        await removeOwnedGeneration(previous, root);
      } else {
        try { await stat(transactionRoot); throw new Error('Experimental slot exists without ownership metadata'); }
        catch (error) { if (error.code !== 'ENOENT') throw error; }
      }
    }
    await mkdir(cargoTarget, { recursive: true });
    await writeMarker(marker);
  } catch (error) {
    await releaseLease(path, identity);
    if (catalogOwned) await releaseLease(catalogPath, identity);
    throw error;
  }
  const childEnvironment = { ...environment, CLEARRA_BUILD_ROOT: root, CLEARRA_BUILD_PURPOSE: purpose,
    CLEARRA_BUILD_SOURCE_ROOT: sourceRoot, CLEARRA_BUILD_SOURCE_ID: sourceId, CLEARRA_BUILD_SESSION_ID: sessionId,
    CLEARRA_BUILD_TRANSACTION_ROOT: transactionRoot, CLEARRA_BUILD_CACHE_OWNER_PID: String(process.pid),
    CLEARRA_BUILD_CACHE_SESSION_KEY: sessionId, CARGO_TARGET_DIR: cargoTarget, CARGO_INCREMENTAL: '0', RUSTC_WRAPPER: guard };
  if (process.platform !== 'win32' && /^[A-Za-z]:\//u.test(buildPathIdentity(root))) childEnvironment.LOCALAPPDATA = buildPathIdentity(root).replace(/\/Clearra\/build$/iu, '').replaceAll('/', '\\');
  let finished = false;
  return { environment: childEnvironment, transaction: marker, finish: async success => {
    if (finished) return;
    assertManagedBuildTransaction({ environment: childEnvironment, sourceRoot });
    finished = true;
    marker.status = success ? 'complete' : 'failed';
    marker.completed_utc = success ? new Date().toISOString() : null;
    try {
      try {
        await writeMarker(marker);
        if (purpose === 'product' && !success) await removeOwnedGeneration(marker, root);
      } finally { await releaseLease(path, identity); }
      if (purpose === 'product' && success) await retainProductBuildGenerations(root);
    } finally { if (catalogOwned) await releaseLease(catalogPath, identity); }
  } };
}
