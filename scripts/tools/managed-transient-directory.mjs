import { mkdir, open, readFile, rm } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';

const LOCK_RETRY_LIMIT = 3;

export async function acquireManagedTransientDirectory(path) {
  const directory = resolve(path);
  const lockPath = `${directory}.lock`;
  await mkdir(dirname(directory), { recursive: true });

  let lock = null;
  for (let attempt = 0; attempt < LOCK_RETRY_LIMIT; attempt += 1) {
    try {
      lock = await open(lockPath, 'wx');
      break;
    } catch (error) {
      if (error?.code !== 'EEXIST') throw error;
      const owner = await readOwner(lockPath);
      if (owner !== null && processIsAlive(owner.pid)) {
        throw new Error(
          `managed transient directory is already active: ${directory} (pid ${owner.pid})`
        );
      }
      await rm(lockPath, { force: true });
    }
  }
  if (lock === null) {
    throw new Error(`could not acquire managed transient directory: ${directory}`);
  }

  try {
    await lock.writeFile(
      `${JSON.stringify({ schema_version: 1, pid: process.pid })}\n`,
      'utf8'
    );
    await rm(directory, { recursive: true, force: true });
    await mkdir(directory, { recursive: true });
  } catch (error) {
    await lock.close();
    await rm(lockPath, { force: true });
    throw error;
  }

  let released = false;
  return {
    path: directory,
    async release({ remove = true } = {}) {
      if (released) return;
      released = true;
      try {
        if (remove) await rm(directory, { recursive: true, force: true });
      } finally {
        await lock.close();
        await rm(lockPath, { force: true });
      }
    }
  };
}

async function readOwner(lockPath) {
  try {
    const value = JSON.parse(await readFile(lockPath, 'utf8'));
    return Number.isInteger(value?.pid) && value.pid > 0 ? value : null;
  } catch {
    return null;
  }
}

function processIsAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error?.code !== 'ESRCH';
  }
}
