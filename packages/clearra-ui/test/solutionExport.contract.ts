import assert from 'node:assert/strict';

import { decoder as fumenDecoder, encoder as fumenEncoder, Field } from 'tetris-fumen';

import { decodeCtk3 } from '../src/lib/workspace/ctk3Codec';
import {
  encodeColoredFumenPages,
  encodeSolutionPages,
  SolutionExportError,
  type SolutionExportPage,
  type SolutionPiece
} from '../src/lib/workspace/solutionExport';
import {
  encodeSolutionKeySourceForClipboard,
  encodeSolutionKeysForClipboard,
  encodeSolutionPagesForClipboard
} from '../src/lib/workspace/solutionExportAsync';
import { encodeFieldDocumentAsync } from '../src/lib/workspace/fieldInterchange';

const pieces: SolutionPiece[] = ['I', 'O', 'T', 'S', 'Z', 'J', 'L'];
const fullPage: SolutionExportPage = {
  height: 4,
  initialMask: 0n,
  placements: Array.from({ length: 10 }, (_, index) => ({
    piece: pieces[index % pieces.length],
    mask: 0xfn << BigInt(index * 4)
  }))
};
const partialPage: SolutionExportPage = {
  height: 4,
  initialMask: 0x3n,
  placements: [
    { piece: 'T', mask: 0x3cn },
    { piece: 'I', mask: 0x3c0n }
  ]
};
const solutionComment = 'PC probability: 50% | Average score: 1200';
const commentedPage: SolutionExportPage = {
  ...partialPage,
  comment: solutionComment
};
const pages = [fullPage, partialPage, partialPage, fullPage];

const expectedFumen = fumenEncoder.encode(
  pages.map((page) => ({
    field: Field.create(fieldText(page)),
    flags: {
      lock: true,
      mirror: false,
      colorize: true,
      rise: false
    }
  }))
);
const actualFumen = encodeColoredFumenPages(pages);
assert.equal(actualFumen, expectedFumen);
assert.equal(fumenDecoder.decode(actualFumen).length, pages.length);

const commentedFumen = encodeColoredFumenPages([commentedPage]);
assert.equal(fumenDecoder.decode(commentedFumen)[0]?.comment, solutionComment);
const commentedCtk = encodeSolutionPages([commentedPage], 'ctk');
assert.equal(decodeCtk3(commentedCtk).pages[0]?.comment, solutionComment);

const key =
  'ctk1|initial=0000000000000000|placements=T:000000000000000f';
const keys = Array<string>(5000).fill(key);
const bundled = await encodeSolutionKeysForClipboard(keys, 'ctk');
const decoded = decodeCtk3(bundled);
assert.equal(decoded.pages.length, keys.length);
assert.match(bundled, /^ctk3b_/);
let sourceReadCount = 0;
let largestSourceRead = 0;
const streamedPageCount = 5000;
const streamed = await encodeSolutionKeySourceForClipboard(
  {
    keyCount: streamedPageCount,
    readKeys(_start, count) {
      sourceReadCount += 1;
      largestSourceRead = Math.max(largestSourceRead, count);
      return Array<string>(count).fill(key);
    }
  },
  'ctk'
);
assert.equal(decodeCtk3(streamed).pages.length, streamedPageCount);
assert.equal(sourceReadCount, 8);
assert.equal(largestSourceRead, 1000);

const commentedFromKey = await encodeSolutionKeySourceForClipboard(
  {
    keyCount: 1,
    readKeys() {
      return [key];
    },
    commentForKey() {
      return solutionComment;
    }
  },
  'ctk'
);
assert.equal(decodeCtk3(commentedFromKey).pages[0]?.comment, solutionComment);
const commentedFumenFromKey = await encodeSolutionKeySourceForClipboard(
  {
    keyCount: 1,
    readKeys() {
      return [key];
    },
    commentForKey() {
      return solutionComment;
    }
  },
  'fumen'
);
assert.equal(
  fumenDecoder.decode(commentedFumenFromKey)[0]?.comment,
  solutionComment
);

const p7p4TilingCount = 10_117_860;
let oversizedCtkReads = 0;
await assert.rejects(
  encodeSolutionKeySourceForClipboard(
    {
      keyCount: p7p4TilingCount,
      readKeys() {
        oversizedCtkReads += 1;
        return [];
      }
    },
    'ctk'
  ),
  isClipboardSizeError
);
assert.equal(oversizedCtkReads, 0);

let oversizedFumenReads = 0;
await assert.rejects(
  encodeSolutionKeySourceForClipboard(
    {
      keyCount: p7p4TilingCount,
      readKeys() {
        oversizedFumenReads += 1;
        return [];
      }
    },
    'fumen'
  ),
  isClipboardSizeError
);
assert.equal(oversizedFumenReads, 0);

const originalWorker = globalThis.Worker;
let terminatedWorkers = 0;
class HangingWorker {
  onerror: ((event: ErrorEvent) => void) | null = null;
  onmessage: ((event: MessageEvent) => void) | null = null;
  postMessage() {}
  terminate() {
    terminatedWorkers += 1;
  }
}
Object.assign(globalThis, { Worker: HangingWorker });
try {
  const controller = new AbortController();
  const pending = encodeSolutionKeysForClipboard(keys, 'ctk', {
    signal: controller.signal
  });
  controller.abort();
  await assert.rejects(
    pending,
    (error: unknown) => error instanceof Error && error.name === 'AbortError'
  );
  const pageController = new AbortController();
  const pendingPages = encodeSolutionPagesForClipboard(
    Array<SolutionExportPage>(5000).fill(partialPage),
    'fumen',
    { signal: pageController.signal }
  );
  pageController.abort();
  await assert.rejects(
    pendingPages,
    (error: unknown) => error instanceof Error && error.name === 'AbortError'
  );
  const documentController = new AbortController();
  const pendingDocument = encodeFieldDocumentAsync(
    {
      width: 10,
      pages: Array.from({ length: 300 }, () => ({
        height: 1,
        cells: ['T', 'T', 'T', 'T', ...Array(6).fill(null)]
      }))
    },
    'fumen',
    { signal: documentController.signal }
  );
  documentController.abort();
  await assert.rejects(
    pendingDocument,
    (error: unknown) => error instanceof Error && error.name === 'AbortError'
  );
  assert.ok(terminatedWorkers > 0);
} finally {
  if (originalWorker) Object.assign(globalThis, { Worker: originalWorker });
  else Reflect.deleteProperty(globalThis, 'Worker');
}

const ordinary = encodeSolutionPages([partialPage], 'ctk');
assert.equal(decodeCtk3(ordinary).pages.length, 1);

console.log(
  JSON.stringify({
    fumen_pages: pages.length,
    ctk_bundle_pages: decoded.pages.length,
    ctk_bundle_characters: bundled.length
  })
);

function fieldText(page: SolutionExportPage): string {
  const cells = Array<string>(page.height * 10).fill('_');
  paint(cells, page.initialMask, 'X');
  for (const placement of page.placements) {
    paint(cells, placement.mask, placement.piece);
  }
  const rows: string[] = [];
  for (let y = page.height - 1; y >= 0; y -= 1) {
    rows.push(cells.slice(y * 10, (y + 1) * 10).join(''));
  }
  return rows.join('');
}

function paint(cells: string[], source: bigint, value: string) {
  let mask = source;
  let index = 0;
  while (mask !== 0n) {
    if (mask & 1n) cells[index] = value;
    mask >>= 1n;
    index += 1;
  }
}

function isClipboardSizeError(error: unknown): boolean {
  return (
    error instanceof SolutionExportError &&
    error.code === 'clipboard-output-too-large'
  );
}
