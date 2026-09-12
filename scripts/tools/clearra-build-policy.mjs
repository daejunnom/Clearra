// One physical build root. Validation never creates or retires a generation.
import { createHash } from 'node:crypto';
import { lstatSync, readFileSync } from 'node:fs';
import { dirname, isAbsolute, resolve, win32 } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const scriptRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
export const BUILD_TRANSACTION_MARKER = '.clearra-build-transaction.json';
export const FORBIDDEN_BUILD_ALIASES = ['CARGO_BUILD_TARGET_DIR', 'CARGO_BUILD_BUILD_DIR', 'CARGO_BUILD_RUSTC_WRAPPER',
  'RUSTC_WORKSPACE_WRAPPER', 'CLEARRA_WSL_CARGO_TARGET_DIR', 'CLEARRA_RELEASE_BUILD_ROOT',
  'CLEARRA_WSL_NATIVE_BUILD_ROOT', 'CLEARRA_CORE_C_BUILD_DIR'];
const fields = {
  purpose: 'CLEARRA_BUILD_PURPOSE', source_root: 'CLEARRA_BUILD_SOURCE_ROOT',
  source_id: 'CLEARRA_BUILD_SOURCE_ID', session_id: 'CLEARRA_BUILD_SESSION_ID',
  transaction_root: 'CLEARRA_BUILD_TRANSACTION_ROOT', cargo_target_dir: 'CARGO_TARGET_DIR',
  owner_pid: 'CLEARRA_BUILD_CACHE_OWNER_PID',
};
const pathFields = new Set(['source_root', 'transaction_root', 'cargo_target_dir']);

export function buildPathIdentity(value) {
  if (typeof value !== 'string' || !value || value.includes('\0')) throw new Error('Clearra build path is missing or invalid');
  if (/^[A-Za-z]:[\\/]/u.test(value)) return win32.resolve(value).replaceAll('\\', '/').toLowerCase();
  const mounted = /^\/mnt\/([a-z])\/(.*)$/u.exec(value);
  if (mounted) return win32.resolve(`${mounted[1]}:/${mounted[2]}`).replaceAll('\\', '/').toLowerCase();
  if (!isAbsolute(value)) throw new Error('Clearra build paths must be absolute');
  return resolve(value);
}
export function buildSourceId(value) {
  return createHash('sha256').update(buildPathIdentity(value)).digest('hex').slice(0, 24);
}
export function nativeBuildPath(value, platform = process.platform) {
  if (platform !== 'win32' && /^[A-Za-z]:[\\/]/u.test(value)) {
    const normalized = win32.resolve(value).replaceAll('\\', '/');
    return `/mnt/${normalized[0].toLowerCase()}/${normalized.slice(3)}`;
  }
  return resolve(value);
}
export function canonicalBuildRoot(environment = process.env, platform = process.platform) {
  if (platform === 'win32') {
    if (!environment.LOCALAPPDATA) throw new Error('LOCALAPPDATA is required for the fixed Clearra build root');
    return win32.resolve(environment.LOCALAPPDATA, 'Clearra', 'build');
  }
  if (environment.WSL_DISTRO_NAME || environment.WSL_INTEROP) {
    if (/^[A-Za-z]:[\\/]/u.test(environment.LOCALAPPDATA ?? '')) return nativeBuildPath(win32.resolve(environment.LOCALAPPDATA, 'Clearra', 'build'), platform);
    const result = spawnSync('/mnt/c/Windows/System32/cmd.exe', ['/d', '/c', 'echo', '%LOCALAPPDATA%'], { encoding: 'utf8', windowsHide: true });
    const base = result.stdout?.trim();
    if (result.status !== 0 || !/^[A-Za-z]:[\\/]/u.test(base ?? '')) throw new Error('Cannot resolve the Windows build root from WSL; no separate Linux cache fallback is allowed');
    return nativeBuildPath(win32.resolve(base, 'Clearra', 'build'), platform);
  }
  const base = environment.XDG_CACHE_HOME || (environment.HOME && resolve(environment.HOME, '.cache'));
  if (!base) throw new Error('A canonical user cache directory is required');
  return resolve(base, 'Clearra', 'build');
}
export function assertBuildPathWithin(path, root) {
  const candidate = buildPathIdentity(path);
  const parent = buildPathIdentity(root).replace(/\/$/u, '');
  if (candidate !== parent && !candidate.startsWith(`${parent}/`)) throw new Error(`Clearra refuses a build path outside its selected root: ${path}`);
}
export function assertNoBuildLinks(path) {
  let cursor = resolve(path);
  for (;;) {
    try { if (lstatSync(cursor).isSymbolicLink()) throw new Error(`Clearra build path traverses a symlink or junction: ${cursor}`); }
    catch (error) { if (error.code !== 'ENOENT') throw error; }
    const parent = dirname(cursor);
    if (cursor === parent) return;
    cursor = parent;
  }
}
export function assertBuildRecord(marker, root, directory) {
  const expectedFields = ['schema_version', 'purpose', 'source_root', 'source_id', 'session_id', 'transaction_root',
    'cargo_target_dir', 'owner_pid', 'status', 'created_utc', 'completed_utc'].sort();
  if (!marker || Object.keys(marker).sort().join('|') !== expectedFields.join('|') || marker.schema_version !== 3 ||
      !['experiment', 'product'].includes(marker.purpose) || !['active', 'complete', 'failed'].includes(marker.status) ||
      !Number.isSafeInteger(marker.owner_pid) || marker.owner_pid < 1 || !/^[a-f0-9]{32}$/u.test(marker.session_id) ||
      marker.source_id !== buildSourceId(marker.source_root)) throw new Error('Invalid Clearra build ownership metadata');
  const actual = buildPathIdentity(directory);
  const rootId = buildPathIdentity(root);
  const name = actual.slice(actual.lastIndexOf('/') + 1);
  const expected = marker.purpose === 'experiment' ? `${rootId}/experiments/${marker.source_id}/current` : `${rootId}/products/${name}`;
  if (actual !== expected || buildPathIdentity(marker.transaction_root) !== expected ||
      buildPathIdentity(marker.cargo_target_dir) !== `${expected}/cargo-target` ||
      (marker.purpose === 'product' && !new RegExp(`^[0-9]{8}t[0-9]{9}z-${marker.session_id}$`, 'iu').test(name))) {
    throw new Error('Clearra build generation path/session identity mismatch');
  }
  const created = Date.parse(marker.created_utc);
  const completed = Date.parse(marker.completed_utc);
  if (!Number.isFinite(created) || (marker.status === 'complete' ? !Number.isFinite(completed) || completed < created : marker.completed_utc !== null)) {
    throw new Error('Invalid Clearra build completion time');
  }
}
export function assertManagedBuildTransaction({ environment = process.env, platform = process.platform, sourceRoot } = {}) {
  for (const name of FORBIDDEN_BUILD_ALIASES) {
    if (environment[name]) throw new Error(`Independent build layout/wrapper override is forbidden: ${name}`);
  }
  const expectedRoot = canonicalBuildRoot(environment, platform);
  if (buildPathIdentity(environment.CLEARRA_BUILD_ROOT) !== buildPathIdentity(expectedRoot)) throw new Error('Clearra build root override is forbidden');
  const transaction = nativeBuildPath(environment.CLEARRA_BUILD_TRANSACTION_ROOT, platform);
  assertBuildPathWithin(transaction, expectedRoot);
  assertNoBuildLinks(transaction);
  const markerPath = resolve(transaction, BUILD_TRANSACTION_MARKER);
  const stat = lstatSync(markerPath);
  if (!stat.isFile() || stat.isSymbolicLink() || stat.size > 16384) throw new Error('Clearra build transaction marker is invalid');
  const marker = JSON.parse(readFileSync(markerPath, 'utf8').replace(/^\uFEFF/u, ''));
  assertBuildRecord(marker, expectedRoot, transaction);
  if (marker.schema_version !== 3 || marker.status !== 'active' || !['experiment', 'product'].includes(marker.purpose)) throw new Error('Clearra requires an active managed build transaction');
  if (environment.CLEARRA_BUILD_CACHE_SESSION_KEY !== marker.session_id) throw new Error('Clearra build session alias binding mismatch');
  for (const [field, name] of Object.entries(fields)) {
    const expected = pathFields.has(field) ? buildPathIdentity(marker[field]) : String(marker[field]);
    const actual = pathFields.has(field) ? buildPathIdentity(environment[name]) : environment[name];
    if (expected !== actual) throw new Error(`Clearra build transaction binding mismatch: ${name}`);
  }
  if (sourceRoot && buildPathIdentity(sourceRoot) !== buildPathIdentity(marker.source_root)) throw new Error('Clearra build source root does not match its owner');
  const sourceId = buildSourceId(marker.source_root);
  if (marker.source_id !== sourceId || !/^[a-f0-9]{32}$/u.test(marker.session_id)) throw new Error('Clearra build purpose/session identity is invalid');
  const rootIdentity = buildPathIdentity(expectedRoot);
  const transactionIdentity = buildPathIdentity(transaction);
  if (marker.purpose === 'experiment' && transactionIdentity !== `${rootIdentity}/experiments/${sourceId}/current`) throw new Error('Clearra experimental builds must use exactly one current slot per source root');
  if (marker.purpose === 'product' && (!transactionIdentity.startsWith(`${rootIdentity}/products/`) || transactionIdentity.slice(rootIdentity.length + '/products/'.length).includes('/'))) throw new Error('Clearra product builds require a direct product generation directory');
  if (buildPathIdentity(marker.cargo_target_dir) !== `${transactionIdentity}/cargo-target`) throw new Error('Clearra Cargo target must be the exact managed transaction target');
  assertNoBuildLinks(nativeBuildPath(marker.cargo_target_dir, platform));
  if (!Number.isSafeInteger(marker.owner_pid) || marker.owner_pid < 1) throw new Error('Clearra build owner PID is invalid');
  const leaseName = marker.purpose === 'experiment' ? `experiment-${marker.source_id}.lock` : `product-${marker.session_id}.lock`;
  const leasePath = resolve(nativeBuildPath(expectedRoot, platform), '.leases', leaseName);
  assertNoBuildLinks(leasePath);
  if (lstatSync(leasePath).size > 16384) throw new Error('Clearra build lease is invalid');
  const lease = JSON.parse(readFileSync(leasePath, 'utf8').replace(/^\uFEFF/u, ''));
  if (lease.schema_version !== 1 || ['purpose', 'source_id', 'session_id', 'owner_pid'].some(key => lease[key] !== marker[key])) throw new Error('Clearra build lease does not match its transaction');
  const crossHost = platform !== 'win32' && /^[A-Za-z]:[\\/]/u.test(marker.transaction_root);
  if (!crossHost) {
    try { process.kill(marker.owner_pid, 0); }
    catch (error) { if (error.code !== 'EPERM') throw new Error('Clearra build owner has exited'); }
  }
  return { ...marker, root: nativeBuildPath(expectedRoot, platform), transaction, cargoTarget: nativeBuildPath(marker.cargo_target_dir, platform) };
}
export function assertCargoOutputArguments(arguments_, transaction) {
  const validate = value => {
    if (value === '-' || value === '') return;
    assertBuildPathWithin(resolve(value), transaction);
    assertNoBuildLinks(resolve(value));
  };
  for (let index = 0; index < arguments_.length; index += 1) {
    const argument = arguments_[index];
    if (['--out-dir', '-o'].includes(argument)) {
      if (!arguments_[index + 1]) throw new Error(`Missing Cargo output path after ${argument}`);
      validate(arguments_[++index]);
    } else if (argument.startsWith('--out-dir=')) validate(argument.slice(10));
    else if (argument.startsWith('-o') && argument.length > 2) validate(argument.slice(2));
    else if (argument === '--emit' || argument.startsWith('--emit=')) {
      const output = argument === '--emit' ? arguments_[++index] : argument.slice(7);
      if (!output) throw new Error('Missing compiler emit argument');
      for (const emission of output.split(',')) {
        const equal = emission.indexOf('=');
        if (equal !== -1) validate(emission.slice(equal + 1));
      }
    } else if (argument === '-C' && arguments_[index + 1]?.startsWith('incremental=')) validate(arguments_[++index].slice(12));
    else if (argument.startsWith('-Cincremental=')) validate(argument.slice(14));
  }
}
export function enterManagedBuildOrRelaunch(sourceRoot, argv = process.argv.slice(1), purpose = process.env.CLEARRA_BUILD_PURPOSE || 'experiment') {
  if (process.env.CLEARRA_BUILD_SESSION_ID) return assertManagedBuildTransaction({ sourceRoot });
  const result = spawnSync(process.execPath, [resolve(scriptRoot, 'scripts/tools/invoke-clearra-build.mjs'), '--source-root', resolve(sourceRoot),
    '--purpose', purpose, '--', process.execPath, ...argv], { stdio: 'inherit', env: process.env, windowsHide: true });
  if (result.error) throw result.error;
  process.exit(result.status ?? 1);
}
