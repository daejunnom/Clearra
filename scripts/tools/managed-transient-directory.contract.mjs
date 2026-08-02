import assert from 'node:assert/strict';
import { access, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { acquireManagedTransientDirectory } from './managed-transient-directory.mjs';

const cacheBase = process.platform === 'win32'
  ? process.env.LOCALAPPDATA || process.env.TEMP || resolve(process.env.USERPROFILE || '.', 'AppData', 'Local')
  : process.env.XDG_CACHE_HOME || resolve(process.env.HOME || '.', '.cache');
const slot = resolve(cacheBase, 'Clearra', 'benchmark-runtime', 'managed-slot-contract');

const first = await acquireManagedTransientDirectory(slot);
await writeFile(resolve(first.path, 'stale.txt'), 'stale', 'utf8');
await first.release({ remove: false });

const second = await acquireManagedTransientDirectory(slot);
try {
  await assert.rejects(access(resolve(second.path, 'stale.txt')));
  await assert.rejects(
    acquireManagedTransientDirectory(slot),
    /already active/
  );
} finally {
  await second.release();
}

console.log('managed_transient_directory_contract=passed');
