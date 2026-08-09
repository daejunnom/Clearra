import assert from 'node:assert/strict';

import { decoder as fumenDecoder, encoder as fumenEncoder, Field } from 'tetris-fumen';

import { decodeCtk3 } from '../src/lib/workspace/ctk3Codec';
import { operationCells } from '../src/lib/workspace/ctkOperationGeometry';
import {
  encodeColoredFumenPages,
  encodeFinesseWitnessCtk,
  encodeSolutionPages,
  finesseWitnessCtkPages,
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

const finesseSolutionKey =
  'ctk1|initial=000000000000003f|placements=I:00000000000003c0,O:0000000000300c00';
const finesseWitness = {
  solutionKey: finesseSolutionKey,
  totalInputs: 3,
  inputSequence: ['hard-drop', 'tap-left', 'hard-drop'],
  placements: [
    { piece: 'I', rotation: 2, x: 6, y: 0 },
    { piece: 'O', rotation: 3, x: 0, y: 0 }
  ]
};
const finesseSource = encodeFinesseWitnessCtk(finesseWitness);
const finesseDocument = decodeCtk3(finesseSource);
assert.equal(finesseDocument.pages.length, 2);
assert.equal(finesseDocument.pages[0]?.comment, 'F=3');
assert.equal(finesseDocument.pages[1]?.comment, undefined);
assert.deepEqual(finesseDocument.pages[0]?.cells.slice(0, 10), [
  'G', 'G', 'G', 'G', 'G', 'G', null, null, null, null
]);
assert.deepEqual(finesseDocument.pages[0]?.operation, {
  piece: 'I', rotation: 'spawn', x: 7, y: 0
});
assert.equal(finesseDocument.pages[1]?.cells.length, 0);
assert.deepEqual(finesseDocument.pages[1]?.operation, {
  piece: 'O', rotation: 'spawn', x: 0, y: 0
});
assert.deepEqual(replayCtkPages(finesseDocument.pages), [
  'O', 'O', null, null, null, null, null, null, null, null,
  'O', 'O', null, null, null, null, null, null, null, null
]);

const patternFinesseDocument = decodeCtk3(encodeFinesseWitnessCtk({
  ...finesseWitness,
  annotationInputs: '3.5000'
}));
assert.equal(patternFinesseDocument.pages[0]?.comment, 'F=3.5');
assert.deepEqual(
  patternFinesseDocument.pages.map((page) => page.operation),
  finesseDocument.pages.map((page) => page.operation)
);
assert.deepEqual(patternFinesseDocument.pages[1]?.cells, finesseDocument.pages[1]?.cells);

const rotationPlacements = [
  {
    witness: { piece: 'O', rotation: 3, x: 0, y: 0 },
    operation: { piece: 'O', rotation: 'spawn', x: 0, y: 0 }
  },
  {
    witness: { piece: 'I', rotation: 3, x: 2, y: 0 },
    operation: { piece: 'I', rotation: 'right', x: 2, y: 2 }
  },
  {
    witness: { piece: 'S', rotation: 2, x: 4, y: 0 },
    operation: { piece: 'S', rotation: 'spawn', x: 5, y: 0 }
  },
  {
    witness: { piece: 'Z', rotation: 3, x: 8, y: 0 },
    operation: { piece: 'Z', rotation: 'right', x: 8, y: 1 }
  }
] as const;
const rotationSolutionKey = `ctk1|initial=${hexMask(0n)}|placements=${rotationPlacements
  .map(({ witness, operation }) =>
    `${witness.piece}:${hexMask(maskForCtkOperation(operation))}`)
  .join(',')}`;
const rotationDocument = decodeCtk3(encodeFinesseWitnessCtk({
  solutionKey: rotationSolutionKey,
  totalInputs: rotationPlacements.length,
  inputSequence: rotationPlacements.map(() => 'hard-drop'),
  placements: rotationPlacements.map(({ witness }) => witness)
}));
assert.deepEqual(
  rotationDocument.pages.map((page) => page.operation),
  rotationPlacements.map(({ operation }) => operation)
);

assert.throws(
  () => finesseWitnessCtkPages({
    ...finesseWitness,
    placements: [
      { piece: 'I', rotation: 0, x: 0, y: 0 },
      finesseWitness.placements[1]
    ]
  }),
  isInvalidFinesseWitness
);
assert.throws(
  () => finesseWitnessCtkPages({
    ...finesseWitness,
    placements: [
      { piece: 'I', rotation: 0, x: 8, y: 0 },
      finesseWitness.placements[1]
    ]
  }),
  isInvalidFinesseWitness
);
assert.throws(
  () => finesseWitnessCtkPages({
    ...finesseWitness,
    inputSequence: ['hard-drop', 'tap-left', 'tap-right']
  }),
  isInvalidFinesseWitness
);
assert.throws(
  () => finesseWitnessCtkPages({
    solutionKey: 'ctk1|initial=0000000000000000|placements=O:0000000000000c03,I:000000000000003c',
    totalInputs: 2,
    inputSequence: ['hard-drop', 'hard-drop'],
    placements: [
      { piece: 'I', rotation: 0, x: 0, y: 0 },
      { piece: 'O', rotation: 0, x: 4, y: 0 }
    ]
  }),
  isFinesseSolutionMismatch
);
assert.throws(
  () => finesseWitnessCtkPages({
    solutionKey:
      'ctk1|initial=00000000000003ff|placements=O:0000000000300c00',
    totalInputs: 1,
    inputSequence: ['hard-drop'],
    placements: [{ piece: 'O', rotation: 0, x: 0, y: 1 }]
  }),
  isInvalidFinesseWitness
);

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

function isInvalidFinesseWitness(error: unknown): boolean {
  return error instanceof SolutionExportError && error.code === 'invalid-finesse-witness';
}

function isFinesseSolutionMismatch(error: unknown): boolean {
  return error instanceof SolutionExportError &&
    error.code === 'finesse-witness-solution-mismatch';
}

function replayCtkPages(pages: ReturnType<typeof decodeCtk3>['pages']) {
  const height = Math.max(
    1,
    ...pages.map((page) => page.height),
    ...pages.flatMap((page) => page.operation
      ? operationCells(page.operation).map((cell) => cell.y + 1)
      : [])
  );
  let rows = Array.from({ length: height }, () => Array<string | null>(10).fill(null));
  for (const page of pages) {
    rows = Array.from({ length: height }, (_, y) =>
      Array.from({ length: 10 }, (_, x) => page.cells[y * 10 + x] ?? null)
    );
    assert.ok(page.operation);
    for (const cell of operationCells(page.operation!)) {
      assert.equal(rows[cell.y]?.[cell.x], null);
      rows[cell.y][cell.x] = page.operation!.piece;
    }
    rows = rows.filter((row) => row.some((cell) => cell === null));
    while (rows.length < height) rows.push(Array<string | null>(10).fill(null));
  }
  while (rows.at(-1)?.every((cell) => cell === null)) rows.pop();
  return rows.flat();
}

function maskForCtkOperation(operation: Parameters<typeof operationCells>[0]): bigint {
  return operationCells(operation).reduce(
    (mask, cell) => mask | (1n << BigInt(cell.y * 10 + cell.x)),
    0n
  );
}

function hexMask(mask: bigint): string {
  return mask.toString(16).padStart(16, '0');
}
