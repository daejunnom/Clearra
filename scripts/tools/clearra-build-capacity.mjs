// Peer build-storage limits count logical bytes. This is bounded observation
// and admission, not a filesystem quota and not deletion authorization.
import { constants } from 'node:fs';
import { lstat, open, opendir } from 'node:fs/promises';
import { basename, relative, resolve, sep } from 'node:path';
import policy from '../../config/build-storage.v1.json' with { type: 'json' };
import {
  BUILD_TRANSACTION_MARKER, assertBuildRecord, assertBuildPathWithin,
  assertNoBuildLinks, buildPathIdentity, canonicalBuildRoot, canonicalNativeBuildPath,
} from './clearra-build-policy.mjs';

export const BUILD_STORAGE_POLICY = Object.freeze(policy);
const UNOWNED = 'E_CLEARRA_BUILD_STORAGE_UNOWNED';
const INCOMPLETE = 'E_CLEARRA_BUILD_STORAGE_INCOMPLETE';
const LIMIT = 'E_CLEARRA_BUILD_STORAGE_LIMIT';
const MAX_METADATA_BYTES = 16_384;
const limitKeys = ['slotBytes', 'catalogBytes', 'completedSlots', 'activeSlots', 'sampleMs', 'maxEntries', 'maxDepth'];
const ownedLease = /^(?:build-storage-catalog|products-catalog|experiment-[a-f0-9]{24}|product-[a-f0-9]{32})\.lock$/u;
const contentForbidden = /^(?:\.env(?:\..*)?|id_(?:rsa|dsa|ecdsa|ed25519)(?:\..*)?|authorized_keys)$/iu;

function failure(code, message, details = undefined) {
  return Object.assign(new Error(`${code}: ${message}`), { code, details });
}

function verifyLimits(limits) {
  for (const key of limitKeys) {
    if (!Number.isSafeInteger(limits?.[key]) || limits[key] < 1) {
      throw failure(INCOMPLETE, 'invalid build storage limits');
    }
  }
  if (limits.slotBytes > limits.catalogBytes || limits.sampleMs > 0x7fff_ffff) {
    throw failure(INCOMPLETE, 'inconsistent build storage limits');
  }
}

function requestedIdentity(path, root) {
  path = canonicalNativeBuildPath(path);
  assertBuildPathWithin(path, root);
  assertNoBuildLinks(path);
  const items = relative(root, path).split(sep);
  if (!(items[0] === 'products' && items.length === 2 &&
        /^[0-9]{8}T[0-9]{9}Z-[a-f0-9]{32}$/u.test(items[1])) &&
      !(items[0] === 'experiments' && items.length === 3 &&
        /^[a-f0-9]{24}$/u.test(items[1]) && items[2] === 'current')) {
    throw failure(UNOWNED, 'requested path is not an exact managed build slot');
  }
  return buildPathIdentity(path);
}

async function readMarker(path) {
  assertNoBuildLinks(path);
  const metadata = await lstat(path);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size > MAX_METADATA_BYTES) {
    throw failure(UNOWNED, 'invalid build slot marker');
  }
  const handle = await open(path, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0));
  try {
    const actual = await handle.stat();
    if (actual.dev !== metadata.dev || actual.ino !== metadata.ino || !actual.isFile()) {
      throw failure(INCOMPLETE, 'build slot marker changed during inspection');
    }
    const sample = Buffer.alloc(MAX_METADATA_BYTES + 1);
    let used = 0;
    while (used < sample.length) {
      const { bytesRead } = await handle.read(sample, used, sample.length - used, used);
      if (bytesRead === 0) break;
      used += bytesRead;
    }
    if (used > MAX_METADATA_BYTES) throw failure(UNOWNED, 'build slot marker exceeds its bound');
    const bytes = sample.subarray(0, used);
    return { bytes, record: JSON.parse(bytes.toString('utf8').replace(/^\uFEFF/u, '')) };
  } finally {
    await handle.close();
  }
}

export async function checkBuildStorage(root, { slot, phase = 'live', limits = BUILD_STORAGE_POLICY } = {}) {
  verifyLimits(limits);
  if (!['admit', 'live', 'complete'].includes(phase) || (phase === 'complete' && !slot)) {
    throw failure(INCOMPLETE, 'invalid storage check phase');
  }
  assertNoBuildLinks(root);
  const requested = slot ? requestedIdentity(slot, root) : undefined;
  let visited = 0;
  let catalogBytes = 0;
  let completed = 0;
  let active = 0;
  let requestSeen = false;
  let requestedBytes = 0;

  function count(depth) {
    if (++visited > limits.maxEntries || depth > limits.maxDepth) {
      throw failure(INCOMPLETE, 'build storage inventory exceeds its finite walk bounds');
    }
  }
  async function metadata(path, depth) {
    count(depth);
    if (contentForbidden.test(basename(path)) ||
        /(?:credential|service[-_]?account|api[-_]?key|\.(?:pem|key|pfx|p12)$)/iu.test(basename(path))) {
      throw failure(UNOWNED, 'credential-like entry in the compiler catalog');
    }
    const entry = await lstat(path);
    if (entry.isSymbolicLink() || !(entry.isFile() || entry.isDirectory())) {
      throw failure(UNOWNED, 'build storage refuses a nested symlink or junction or foreign filesystem entry');
    }
    return entry;
  }
  function sum(left, right, ceiling, subject) {
    const bytes = left + right;
    if (!Number.isSafeInteger(bytes)) throw failure(INCOMPLETE, 'build storage byte count overflow');
    if (bytes > ceiling) throw failure(LIMIT, `${subject} exceeds its logical byte limit`, { bytes, ceiling });
    return bytes;
  }
  async function fileTree(path, depth, transaction, record, siblingMayDisappear, state) {
    let entry;
    try {
      entry = await metadata(path, depth);
      if (entry.isFile()) {
        state.bytes = sum(state.bytes, entry.size, limits.slotBytes, 'build slot');
        catalogBytes = sum(catalogBytes, entry.size, limits.catalogBytes, 'build catalog');
        return;
      }
    } catch (cause) {
      if (cause.code === 'ENOENT' && siblingMayDisappear) return;
      throw cause;
    }
    let openDir;
    try { openDir = await opendir(path); }
    catch (cause) { if (cause.code === 'ENOENT' && siblingMayDisappear) return; throw cause; }
    for await (const child of openDir) {
      // Prior/current generations are never inspected as arbitrary independent trees.
      if ((child.name === BUILD_TRANSACTION_MARKER && path !== transaction) || child.name === '.leases') {
        throw failure(UNOWNED, 'foreign nested build ownership metadata');
      }
      await fileTree(resolve(path, child.name), depth + 1, transaction, record, phase === 'live' &&
        record.status === 'active' && child.name !== BUILD_TRANSACTION_MARKER, state);
    }
  }
  async function slotDirectory(path, depth, expectedPurpose) {
    if (!(await metadata(path, depth)).isDirectory()) throw failure(UNOWNED, 'foreign build slot entry');
    const markerPath = resolve(path, BUILD_TRANSACTION_MARKER);
    const { bytes, record } = await readMarker(markerPath);
    assertBuildRecord(record, root, path);
    if (record.purpose !== expectedPurpose) throw failure(UNOWNED, 'build storage role does not match its record');
    const matching = requested === buildPathIdentity(path);
    if (phase === 'admit' && record.status === 'active') {
      throw failure(UNOWNED, 'an active or interrupted build slot requires explicit review');
    }
    if (matching && phase === 'live' && record.status !== 'active') throw failure(UNOWNED, 'sample does not own an active build slot');
    if (matching && phase === 'complete' && !['active', 'complete'].includes(record.status)) {
      throw failure(UNOWNED, 'completion does not name a valid build slot');
    }
    if (matching && phase === 'admit') active += 1;
    else if (matching && phase === 'complete') completed += 1;
    else if (record.status === 'complete') completed += 1;
    else if (record.status === 'active') active += 1;
    const state = { bytes: 0 };
    for await (const child of await opendir(path)) {
      await fileTree(resolve(path, child.name), depth + 1, path, record,
        phase === 'live' && record.status === 'active' && child.name !== BUILD_TRANSACTION_MARKER, state);
    }
    if (!bytes.equals((await readMarker(markerPath)).bytes)) {
      throw failure(INCOMPLETE, 'build slot owner changed while measuring capacity');
    }
    if (matching) {
      requestSeen = true;
      requestedBytes = state.bytes;
    }
  }
  async function catalog() {
    let rootMetadata;
    try { rootMetadata = await metadata(root, 0); }
    catch (error) { if (error.code === 'ENOENT' && (phase === 'admit' || !requested)) return; throw error; }
    if (!rootMetadata.isDirectory()) throw failure(UNOWNED, 'build catalog is not a directory');
    for await (const branch of await opendir(root)) {
      const directory = resolve(root, branch.name);
      if (!['.leases', 'experiments', 'products'].includes(branch.name) ||
          !(await metadata(directory, 1)).isDirectory()) {
        throw failure(UNOWNED, 'unknown entry in the Clearra compiler catalog; review it explicitly');
      }
      for await (const entry of await opendir(directory)) {
        const path = resolve(directory, entry.name);
        if (branch.name === 'products') await slotDirectory(path, 2, 'product');
        else if (branch.name === 'experiments') {
          if (!/^[a-f0-9]{24}$/u.test(entry.name) || !(await metadata(path, 2)).isDirectory()) {
            throw failure(UNOWNED, 'unknown experimental build purpose');
          }
          for await (const current of await opendir(path)) {
            if (current.name !== 'current') throw failure(UNOWNED, 'unknown experimental slot');
            await slotDirectory(resolve(path, current.name), 3, 'experiment');
          }
        } else {
          if (!ownedLease.test(entry.name)) throw failure(UNOWNED, 'unknown build ownership lease');
          const lease = await metadata(path, 2);
          if (!lease.isFile() || lease.size > MAX_METADATA_BYTES) throw failure(UNOWNED, 'invalid build ownership lease');
          catalogBytes = sum(catalogBytes, lease.size, limits.catalogBytes, 'build catalog');
        }
      }
    }
  }
  try {
    await catalog();
  } catch (cause) {
    if ([UNOWNED, INCOMPLETE, LIMIT].includes(cause.code)) throw cause;
    throw failure(INCOMPLETE, 'catalog observation did not finish; owned bytes cannot be certified');
  }
  if (requested && !requestSeen) {
    if (phase !== 'admit') throw failure(UNOWNED, 'the requested slot is not in the catalog');
    active += 1;
  }
  if (active > limits.activeSlots || completed > limits.completedSlots ||
      (requested && phase === 'admit' && completed + 1 > limits.completedSlots)) {
    throw failure(LIMIT, 'build cannot meet slot limits without retiring a protected or unproven slot', { active, completed });
  }
  return { phase, catalogBytes, requestedBytes, completed, active, visited };
}

// Starts one observation after each interval, not overlapping inventories.
// The caller owns any stop operation; the observer never kills a PID or deletes.
export function monitorBuildStorage(check, onError, { sampleMs = BUILD_STORAGE_POLICY.sampleMs } = {}) {
  let stopped = false;
  let running;
  let timer;
  const schedule = () => {
    timer = setTimeout(() => {
      running = Promise.resolve().then(check).then(() => {
        if (!stopped) schedule();
      }, cause => {
        stopped = true;
        onError(cause);
      });
    }, sampleMs);
    timer.unref?.();
  };
  schedule();
  return async () => {
    stopped = true;
    clearTimeout(timer);
    await running;
  };
}

if (import.meta.main) {
  try {
    const [command, ...args] = process.argv.slice(2);
    if (command !== 'check' || args.length % 2) throw new Error('expected check --root [--phase] [--slot]');
    const options = Object.create(null);
    for (let index = 0; index < args.length; index += 2) {
      const key = args[index];
      if (!['--root', '--slot', '--phase'].includes(key) || Object.hasOwn(options, key)) throw new Error('invalid storage check option');
      options[key] = args[index + 1];
    }
    const root = canonicalBuildRoot();
    if (buildPathIdentity(options['--root']) !== buildPathIdentity(root)) throw failure(UNOWNED, 'build check requires the canonical root');
    const result = await checkBuildStorage(root, { slot: options['--slot'], phase: options['--phase'] ?? 'live' });
    process.stdout.write(`${JSON.stringify(result)}\n`);
  } catch (error) {
    process.stderr.write(`${JSON.stringify({ code: error.code ?? INCOMPLETE, message: error.message, ...error.details })}\n`);
    process.exitCode = 1;
  }
}
