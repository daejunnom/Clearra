import { randomUUID } from 'node:crypto';
import { lstat, mkdir, open, readdir, readFile, rename, rm, writeFile } from 'node:fs/promises';
import { basename, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { assertBuildPathWithin, assertManagedBuildTransaction, assertNoBuildLinks } from './clearra-build-policy.mjs';

const SCHEMA_VERSION = 2;
const RECORD_LIMIT = 8192;
const TREE_ENTRY_LIMIT = 100000;
const TREE_DEPTH_LIMIT = 64;

// Interrupted leases require reviewed recovery: PID disappearance alone does
// not prove that descendants stopped. Sidecars stay outside staged payloads.
export async function acquireManagedTransientDirectory(path, { ownerPid = process.pid } = {}) {
  if (![process.pid, process.ppid].includes(ownerPid) || !Number.isSafeInteger(ownerPid) || ownerPid < 1) {
    throw new Error('transient lease can belong only to this process or its verified parent');
  }
  const directory = resolve(path);
  if (directory === dirname(directory) || dirname(directory) === dirname(dirname(directory))) {
    throw new Error('managed transient directory must be a specific owned slot');
  }
  const lockPath = `${directory}.lock`;
  const recordPath = `${directory}.owner.json`;
  const legacyLockPath = resolve(dirname(directory), `.${basename(directory)}.lock`);
  assertNoBuildLinks(directory);
  assertNoBuildLinks(lockPath);
  assertNoBuildLinks(recordPath);
  assertNoBuildLinks(legacyLockPath);
  if (await pathExists(legacyLockPath)) {
    throw new Error(`managed transient directory has an unresolved legacy lease; explicit reviewed recovery required: ${legacyLockPath}`);
  }
  await mkdir(dirname(directory), { recursive: true });
  assertNoBuildLinks(directory);

  let lock;
  try {
    lock = await open(lockPath, 'wx');
  } catch (error) {
    if (error?.code !== 'EEXIST') throw error;
    const owner = await readRecord(lockPath);
    if (validRecord(owner, directory, 'active') && processIsAlive(owner.pid)) {
      throw new Error(`managed transient directory is already active: ${directory} (pid ${owner.pid})`);
    }
    throw new Error(`managed transient directory has an unresolved or invalid lease; explicit reviewed recovery required: ${directory}`);
  }

  const identity = {
    schema_version: SCHEMA_VERSION, directory, platform: process.platform,
    arch: process.arch, pid: ownerPid, token: randomUUID(), state: 'active'
  };
  try {
    await lock.writeFile(`${JSON.stringify(identity)}\n`, 'utf8');
    const previous = await readRecord(recordPath);
    const exists = await pathExists(directory);
    if ((previous !== null || exists) && !validRecord(previous, directory, 'complete')) {
      throw new Error(`managed transient directory has unknown or interrupted ownership; explicit reviewed recovery required: ${directory}`);
    }
    if (exists) await assertOwnedTree(directory);
    await writeRecord(recordPath, identity);
    if (exists) await rm(directory, { recursive: true });
    await mkdir(directory);
  } catch (error) {
    await lock.close();
    // Remove only this acquisition's lease, never an unknown/replaced lock.
    await removeMatchingRecord(lockPath, identity);
    throw error;
  }

  let released = false;
  return {
    path: directory,
    identity,
    async handoffToParent() {
      if (ownerPid !== process.ppid || ownerPid === process.pid || released) {
        throw new Error('transient handoff requires the verified parent owner');
      }
      await assertMatchingRecord(lockPath, identity);
      await assertMatchingRecord(recordPath, identity);
      released = true;
      await lock.close();
    },
    // Call after draining owned consumers. This utility neither infers
    // descendant liveness nor kills processes on the caller's behalf.
    async release({ remove = true } = {}) {
      if (released) return;
      released = true;
      try {
        await releaseDirectory(directory, identity, remove);
      } finally {
        // Cleanup/identity failure preserves the unresolved lease.
        await lock.close();
      }
    }
  };
}

// An outer build owner must not retire an inner unresolved staging lease even
// when the outer command has returned. Inspect direct scratch slots only, not
// Cargo's private lock files inside cargo-target.
export async function assertManagedTransientSlotsRetired(root) {
  root = resolve(root);
  assertNoBuildLinks(root);
  await assertRetiredSlots(root);
  const nested = resolve(root, 'transient');
  if (await pathExists(nested)) {
    assertNoBuildLinks(nested);
    await assertRetiredSlots(nested);
  }
}

async function assertRetiredSlots(root) {
  for (const name of await readdir(root)) {
    if (name.endsWith('.lock') || name.endsWith('.owner.json.pending')) {
      throw new Error(`unresolved build scratch lease; retirement refused: ${resolve(root, name)}`);
    }
    if (name.endsWith('.owner.json')) {
      const path = resolve(root, name);
      const directory = path.slice(0, -'.owner.json'.length);
      if (!validRecord(await readRecord(path), directory, 'complete')) {
        throw new Error(`unknown or interrupted build scratch ownership; retirement refused: ${path}`);
      }
    }
  }
}

async function releaseDirectory(directory, identity, remove) {
  const lockPath = `${directory}.lock`;
  const recordPath = `${directory}.owner.json`;
  await assertMatchingRecord(lockPath, identity);
  await assertMatchingRecord(recordPath, identity);
  if (remove) {
    if (await pathExists(directory)) {
      await assertOwnedTree(directory);
      await rm(directory, { recursive: true });
    }
    await removeMatchingRecord(recordPath, identity);
  } else {
    await writeRecord(recordPath, { ...identity, state: 'complete' });
  }
  await removeMatchingRecord(lockPath, identity);
}

async function pathExists(path) {
  try { await lstat(path); return true; }
  catch (error) { if (error?.code === 'ENOENT') return false; throw error; }
}

async function readRecord(path) {
  try {
    assertNoBuildLinks(path);
    const stat = await lstat(path);
    if (!stat.isFile() || stat.size > RECORD_LIMIT) return { invalid: true };
    const record = JSON.parse(await readFile(path, 'utf8'));
    return record !== null && typeof record === 'object' ? record : { invalid: true };
  } catch (error) {
    return error?.code === 'ENOENT' ? null : { invalid: true };
  }
}

function validRecord(record, directory, state) {
  return record !== null && record?.schema_version === SCHEMA_VERSION &&
    record.directory === directory && record.platform === process.platform &&
    record.arch === process.arch && record.state === state &&
    Number.isSafeInteger(record.pid) && record.pid > 0 &&
    /^[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}$/u.test(record.token ?? '') &&
    Object.keys(record).sort().join('|') === 'arch|directory|pid|platform|schema_version|state|token';
}

async function assertMatchingRecord(path, identity) {
  const record = await readRecord(path);
  if (!validRecord(record, identity.directory, identity.state) || record.token !== identity.token || record.pid !== identity.pid) {
    throw new Error(`managed transient directory ownership changed; cleanup refused: ${path}`);
  }
}

async function removeMatchingRecord(path, identity) {
  await assertMatchingRecord(path, identity);
  await rm(path);
}

async function writeRecord(path, record) {
  const pending = `${path}.pending`;
  assertNoBuildLinks(path);
  assertNoBuildLinks(pending);
  // Interrupted pending metadata is not disposable based on PID alone.
  await writeFile(pending, `${JSON.stringify(record)}\n`, { encoding: 'utf8', flag: 'wx' });
  await rename(pending, path);
}

async function assertOwnedTree(root) {
  assertNoBuildLinks(root);
  let entries = 0;
  const deadline = Date.now() + 5000;
  const walk = async (path, depth) => {
    if (++entries > TREE_ENTRY_LIMIT || depth > TREE_DEPTH_LIMIT || Date.now() > deadline) {
      throw new Error(`managed transient directory cleanup inventory exceeds its bound: ${root}`);
    }
    const stat = await lstat(path);
    if (stat.isSymbolicLink()) throw new Error(`managed transient directory contains a symlink or junction; cleanup refused: ${path}`);
    if (stat.isDirectory()) {
      for (const name of await readdir(path)) await walk(resolve(path, name), depth + 1);
    }
  };
  await walk(root, 0);
}

function processIsAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error?.code !== 'ESRCH';
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [mode, path, token] = process.argv.slice(2);
  if (mode === '--check-retired' && process.argv.length === 4) {
    await assertManagedTransientSlotsRetired(path);
  } else if ((mode === '--acquire-for-parent' && process.argv.length === 4) ||
      (mode === '--release-for-parent' && process.argv.length === 5)) {
    const transaction = assertManagedBuildTransaction();
    const directory = resolve(path);
    assertBuildPathWithin(directory, transaction.transaction);
    if (dirname(directory) !== resolve(transaction.transaction, 'transient')) {
      throw new Error('parent-owned scratch must be a direct purpose slot in the verified build transaction');
    }
    if (mode === '--acquire-for-parent') {
      const lease = await acquireManagedTransientDirectory(directory, { ownerPid: process.ppid });
      await lease.handoffToParent();
      console.log(JSON.stringify(lease.identity));
    } else {
      const identity = await readRecord(`${directory}.lock`);
      if (!validRecord(identity, directory, 'active') || identity.pid !== process.ppid || identity.token !== token) {
        throw new Error('parent-owned scratch identity changed; cleanup refused');
      }
      await releaseDirectory(directory, identity, true);
    }
  } else {
    throw new Error('usage: managed-transient-directory.mjs --check-retired <root> | --acquire-for-parent <slot> | --release-for-parent <slot> <token>');
  }
}
