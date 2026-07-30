import { performance } from 'node:perf_hooks';

import {
  encodeColoredFumenSolutionKeys
} from '../src/lib/workspace/solutionExport';
import { encodeSolutionKeysForClipboard } from '../src/lib/workspace/solutionExportAsync';

const mode = process.argv[2] ?? 'ctk';
const pageCount = parsePageCount(process.argv[3]);
const pieces = ['I', 'O', 'T', 'S', 'Z', 'J', 'L'];
const masks = Array.from({ length: 10 }, (_, index) =>
  (0xfn << BigInt(index * 4)).toString(16).padStart(16, '0')
);
const keys = new Array<string>(pageCount);
for (let index = 0; index < pageCount; index += 1) {
  let value = index;
  const placements = new Array<string>(10);
  for (let placement = 0; placement < placements.length; placement += 1) {
    placements[placement] = `${pieces[value % pieces.length]}:${masks[placement]}`;
    value = Math.floor(value / pieces.length);
  }
  keys[index] =
    `ctk1|initial=0000000000000000|placements=${placements.join(',')}`;
}

const before = process.memoryUsage();
const started = performance.now();
const encoded =
  mode === 'fumen'
    ? encodeColoredFumenSolutionKeys(keys)
    : await encodeSolutionKeysForClipboard(keys, 'ctk');
const elapsedMs = performance.now() - started;
const after = process.memoryUsage();

console.log(
  JSON.stringify({
    mode,
    page_count: pageCount,
    elapsed_ms: Number(elapsedMs.toFixed(2)),
    encoded_characters: encoded.length,
    heap_delta_mib: Number(
      ((after.heapUsed - before.heapUsed) / 1024 / 1024).toFixed(2)
    ),
    rss_delta_mib: Number(((after.rss - before.rss) / 1024 / 1024).toFixed(2))
  })
);

function parsePageCount(value: string | undefined): number {
  if (value === undefined) return 208_437;
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed) || parsed < 1 || parsed > 1_048_576) {
    throw new Error('page count must be an integer between 1 and 1,048,576');
  }
  return parsed;
}
