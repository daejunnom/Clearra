// Restrict read-only analysis to this benchmark's owned receipt directory.
// A caller-supplied path must never turn a report command into a credential reader.
import fs from 'node:fs';
import { basename, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repository = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const receiptRoot = resolve(repository, '_local/artifacts/v081-accelerator-abba');

export function readBenchmarkSummary(path) {
  if (basename(path) !== 'summary.json') {
    throw new Error('only v0.8.1 accelerator summary.json receipts may be read');
  }
  const physical = fs.realpathSync(path);
  const runDirectory = dirname(physical);
  const runId = basename(runDirectory);
  const physicalRoot = fs.realpathSync(receiptRoot);
  const expectedRoot = resolve(fs.realpathSync(repository),
    '_local/artifacts/v081-accelerator-abba');
  if (!/^[a-z0-9][a-z0-9-]{0,47}$/.test(runId) ||
      physicalRoot !== expectedRoot || dirname(runDirectory) !== physicalRoot ||
      basename(physical) !== 'summary.json') {
    throw new Error('receipt must belong to one direct benchmark run directory');
  }
  const info = fs.statSync(physical);
  if (!info.isFile() || info.size > 16 * 1024 * 1024) {
    throw new Error('benchmark receipt is not a bounded regular file');
  }
  return JSON.parse(fs.readFileSync(physical, 'utf8'));
}
