import {
  encodeCtk3,
  encodeCtk3Bundle,
  encodeCtk3Compact,
  FumenCommentCodecError,
  FUMEN_MAX_PAGES,
  type Ctk3Color,
  type Ctk3Operation,
  type Ctk3Page
} from './ctk3Codec';
import { operationCells, operationOffsets } from './ctkOperationGeometry';
import { encodeFastColoredFumenPages } from './fastFumenSolutionEncoder';

export type SolutionCopyFormat = 'fumen' | 'ctk';
export type SolutionPiece = 'I' | 'O' | 'T' | 'S' | 'Z' | 'J' | 'L';
export type SolutionBoardCell = SolutionPiece | 'G' | null;

export type SolutionExportPlacement = {
  piece: SolutionPiece;
  mask: bigint;
};

export type SolutionExportPage = {
  height: number;
  initialMask: bigint;
  placements: SolutionExportPlacement[];
  comment?: string;
};

export type SolutionExportBoard = {
  cells: SolutionBoardCell[];
  height: number;
  page: SolutionExportPage;
};

export type FinesseWitnessPlacement = {
  piece: string;
  rotation: number;
  x: number;
  y: number;
};

export type FinesseWitnessExport = {
  solutionKey: string;
  totalInputs: number;
  annotationInputs?: string | number;
  inputSequence: readonly string[];
  placements: readonly FinesseWitnessPlacement[];
};

export type SolutionExportErrorCode =
  | 'invalid-page'
  | 'invalid-solution-key'
  | 'clipboard-output-too-large'
  | 'fumen-height-unsupported'
  | 'fumen-comment-too-long'
  | 'invalid-fumen-comment'
  | 'fumen-page-limit'
  | 'fumen-roundtrip-mismatch'
  | 'invalid-finesse-witness'
  | 'finesse-witness-solution-mismatch';

export class SolutionExportError extends Error {
  readonly code: SolutionExportErrorCode;

  constructor(code: SolutionExportErrorCode) {
    super(code);
    this.code = code;
  }
}

const BOARD_WIDTH = 10;
const FUMEN_HEIGHT = 23;
const CTK_HEIGHT = 31;
export const CTK_SOLUTION_SEGMENT_SIZE = 1024;
const COMPACT_KEY_PATTERN = /^ctk1\|initial=([0-9a-f]{16})\|placements=(.*)$/;
const EXTENDED_KEY_PATTERN =
  /^ctk2\|height=([0-9]{1,2})\|initial=([0-9a-f]{64})\|placements=(.*)$/;
const COMPACT_PLACEMENT_PATTERN = /^([IOTSZJL]):([0-9a-f]{16})$/;
const EXTENDED_PLACEMENT_PATTERN = /^([IOTSZJL]):([0-9a-f]{64})$/;
const FINESSE_INPUTS = new Set([
  'hold',
  'tap-left',
  'tap-right',
  'das-left',
  'das-right',
  'rotate-clockwise',
  'rotate-counter-clockwise',
  'rotate-180',
  'soft-drop',
  'hard-drop'
]);
const CTK_ROTATIONS = ['spawn', 'right', 'reverse', 'left'] as const;
const FINESSE_SHAPES: Record<SolutionPiece, readonly (readonly [number, number])[][]> = {
  I: [
    [[0, 0], [1, 0], [2, 0], [3, 0]],
    [[0, 0], [0, 1], [0, 2], [0, 3]],
    [[0, 0], [1, 0], [2, 0], [3, 0]],
    [[0, 0], [0, 1], [0, 2], [0, 3]]
  ],
  O: Array.from({ length: 4 }, () => [[0, 0], [1, 0], [0, 1], [1, 1]]),
  T: [
    [[0, 0], [1, 0], [2, 0], [1, 1]],
    [[0, 0], [0, 1], [0, 2], [1, 1]],
    [[0, 1], [1, 1], [2, 1], [1, 0]],
    [[1, 0], [1, 1], [1, 2], [0, 1]]
  ],
  S: [
    [[0, 0], [1, 0], [1, 1], [2, 1]],
    [[1, 0], [0, 1], [1, 1], [0, 2]],
    [[0, 0], [1, 0], [1, 1], [2, 1]],
    [[1, 0], [0, 1], [1, 1], [0, 2]]
  ],
  Z: [
    [[1, 0], [2, 0], [0, 1], [1, 1]],
    [[0, 0], [0, 1], [1, 1], [1, 2]],
    [[1, 0], [2, 0], [0, 1], [1, 1]],
    [[0, 0], [0, 1], [1, 1], [1, 2]]
  ],
  J: [
    [[0, 1], [0, 0], [1, 0], [2, 0]],
    [[0, 0], [0, 1], [0, 2], [1, 2]],
    [[0, 1], [1, 1], [2, 1], [2, 0]],
    [[0, 0], [1, 0], [1, 1], [1, 2]]
  ],
  L: [
    [[2, 1], [0, 0], [1, 0], [2, 0]],
    [[0, 0], [0, 1], [0, 2], [1, 0]],
    [[0, 1], [1, 1], [2, 1], [0, 0]],
    [[0, 2], [1, 0], [1, 1], [1, 2]]
  ]
};

export function parseSolutionKey(key: string): SolutionExportPage | null {
  const compact = COMPACT_KEY_PATTERN.exec(key);
  const extended = compact ? null : EXTENDED_KEY_PATTERN.exec(key);
  if (!compact && !extended) return null;

  const height = extended ? Number(extended[1]) : 1;
  if (!Number.isInteger(height) || height < 1 || height > 24) return null;
  const initialHex = compact ? compact[1] : extended![2];
  const encoded = compact ? compact[2] : extended![3];
  const bitLimit = compact ? 64 : height * BOARD_WIDTH;
  const placementLimit = compact ? 16 : 60;
  const placementPattern = compact
    ? COMPACT_PLACEMENT_PATTERN
    : EXTENDED_PLACEMENT_PATTERN;
  const initialMask = BigInt(`0x${initialHex}`);
  if (initialMask >> BigInt(bitLimit)) return null;
  const encodedPlacements = encoded ? encoded.split(',') : [];
  if (encodedPlacements.length > placementLimit) return null;

  const placements: SolutionExportPlacement[] = [];
  let occupied = initialMask;
  for (const value of encodedPlacements) {
    const placement = placementPattern.exec(value);
    if (!placement) return null;
    const mask = BigInt(`0x${placement[2]}`);
    if (
      mask === 0n ||
      mask >> BigInt(bitLimit) ||
      popcount(mask) !== 4 ||
      (occupied & mask) !== 0n
    ) {
      return null;
    }
    occupied |= mask;
    placements.push({ mask, piece: placement[1] as SolutionPiece });
  }
  return {
    height: compact ? Math.max(1, highestOccupiedRow(occupied) + 1) : height,
    initialMask,
    placements
  };
}

export function renderSolutionBoard(
  page: SolutionExportPage,
  minimumHeight = page.height
): SolutionExportBoard {
  validatePage(page);
  const requestedHeight = Math.max(1, Math.min(24, Math.trunc(minimumHeight || 1)));
  const height = Math.max(
    requestedHeight,
    page.height,
    highestOccupiedRow(occupiedMask(page)) + 1
  );
  const cells = Array<SolutionBoardCell>(height * BOARD_WIDTH).fill(null);
  paintDisplayMask(cells, height, page.initialMask, 'G');
  for (const placement of page.placements) {
    paintDisplayMask(cells, height, placement.mask, placement.piece);
  }
  return { cells, height, page };
}

export function encodeSolution(
  page: SolutionExportPage,
  format: SolutionCopyFormat
): string {
  return encodeSolutionPages([page], format);
}

export function encodeSolutionPages(
  pages: SolutionExportPage[],
  format: SolutionCopyFormat
): string {
  if (!pages.length) throw new SolutionExportError('invalid-page');
  return format === 'fumen'
    ? encodeColoredFumenPages(pages)
    : encodeCtk3({
        width: BOARD_WIDTH,
        pages: pages.map(solutionPageToCtk3Page)
      });
}

export function encodeCtkSolutionKeySegment(keys: readonly string[]): string {
  if (!keys.length || keys.length > CTK_SOLUTION_SEGMENT_SIZE) {
    throw new SolutionExportError('invalid-page');
  }
  const pages = new Array<Ctk3Page>(keys.length);
  for (let index = 0; index < keys.length; index += 1) {
    const page = parseSolutionKey(keys[index]);
    if (!page) throw new SolutionExportError('invalid-solution-key');
    pages[index] = solutionPageToCtk3PageUnchecked(page);
  }
  return encodeCtk3Compact({ width: BOARD_WIDTH, pages });
}

export function combineCtkSolutionSegments(
  segments: readonly string[]
): string {
  return encodeCtk3Bundle(segments);
}

export function encodeColoredFumenSolutionKeys(
  keys: readonly string[]
): string {
  if (!keys.length) throw new SolutionExportError('invalid-page');
  if (keys.length > FUMEN_MAX_PAGES) {
    throw new SolutionExportError('fumen-page-limit');
  }
  return encodeFastColoredFumenPages(parsedFumenPages(keys));
}

export function encodeColoredFumen(page: SolutionExportPage): string {
  return encodeColoredFumenPages([page]);
}

export function encodeCtk(page: SolutionExportPage): string {
  return encodeCtk3({ width: BOARD_WIDTH, pages: [solutionPageToCtk3Page(page)] });
}

/**
 * Encodes the engine's representative route. The typed finesse
 * report remains authoritative; this document carries only the replayable
 * placement sequence and its total input count.
 */
export function encodeFinesseWitnessCtk(witness: FinesseWitnessExport): string {
  return encodeCtk3({
    width: BOARD_WIDTH,
    pages: finesseWitnessCtkPages(witness)
  });
}

export function finesseWitnessCtkPages(
  witness: FinesseWitnessExport
): Ctk3Page[] {
  validateFinesseWitnessEnvelope(witness);
  const annotationInputs = finesseAnnotationInputs(witness.annotationInputs, witness.totalInputs);
  const selected = parseSolutionKey(witness.solutionKey);
  if (!selected) throw new SolutionExportError('invalid-finesse-witness');

  const selectedPieces = selected.placements
    .map((placement) => placement.piece)
    .sort()
    .join('');
  const witnessPieces = witness.placements
    .map((placement) => placement.piece)
    .sort()
    .join('');
  if (selectedPieces !== witnessPieces) {
    throw new SolutionExportError('finesse-witness-solution-mismatch');
  }

  const expectedOccupied = occupiedMask(selected);
  let maximumY = highestOccupiedRow(expectedOccupied);
  const steps = witness.placements.map((placement) => {
    if (!isSolutionPiece(placement.piece) ||
      !Number.isInteger(placement.rotation) || placement.rotation < 0 || placement.rotation > 3 ||
      !Number.isInteger(placement.x) || !Number.isInteger(placement.y)) {
      throw new SolutionExportError('invalid-finesse-witness');
    }
    const cells = FINESSE_SHAPES[placement.piece][placement.rotation].map(
      ([dx, dy]) => ({ x: placement.x + dx, y: placement.y + dy })
    );
    for (const cell of cells) {
      if (cell.x < 0 || cell.x >= BOARD_WIDTH || cell.y < 0) {
        throw new SolutionExportError('invalid-finesse-witness');
      }
      maximumY = Math.max(maximumY, cell.y);
    }
    return {
      cells,
      operation: finesseCtkOperation(placement.piece, placement.rotation, cells),
      piece: placement.piece
    };
  });

  const height = Math.max(selected.height, maximumY + 1, 1);
  if (height > CTK_HEIGHT || selected.initialMask >> BigInt(height * BOARD_WIDTH)) {
    throw new SolutionExportError('invalid-finesse-witness');
  }

  const rows = emptyColoredRows(height);
  paintRowsMask(rows, selected.initialMask, 'G');
  if (fullColoredRows(rows).length !== 0) {
    // Finesse search operates on the once-normalized initial field. Accepting
    // an uncleared row here would make every following operation ambiguous.
    throw new SolutionExportError('invalid-finesse-witness');
  }

  const pages: Ctk3Page[] = [];
  for (let index = 0; index < steps.length; index += 1) {
    const step = steps[index];
    for (const cell of step.cells) {
      if (cell.y >= height || rows[cell.y][cell.x] !== null) {
        throw new SolutionExportError('invalid-finesse-witness');
      }
    }
    pages.push({
      height,
      cells: rows.flat(),
      ...(index === 0 ? { comment: `F=${annotationInputs}` } : {}),
      operation: step.operation,
      flags: {
        lock: true,
        mirror: false,
        colorize: true,
        rise: false,
        quiz: false
      }
    });
    for (const cell of step.cells) rows[cell.y][cell.x] = step.piece;
    const clearedRows = fullColoredRows(rows);
    if (clearedRows.length > 4) {
      throw new SolutionExportError('invalid-finesse-witness');
    }
    clearColoredRows(rows, clearedRows);
  }

  const expectedRows = emptyColoredRows(height);
  paintRowsMask(expectedRows, selected.initialMask, 'G');
  for (const placement of selected.placements) {
    paintRowsMask(expectedRows, placement.mask, placement.piece);
  }
  clearColoredRows(expectedRows, fullColoredRows(expectedRows));
  if (!sameColoredRows(rows, expectedRows)) {
    throw new SolutionExportError('finesse-witness-solution-mismatch');
  }
  return pages;
}

export function encodeColoredFumenPages(pages: SolutionExportPage[]): string {
  if (!pages.length) throw new SolutionExportError('invalid-page');
  if (pages.length > FUMEN_MAX_PAGES) {
    throw new SolutionExportError('fumen-page-limit');
  }
  for (const page of pages) {
    validatePage(page);
    const height = Math.max(1, highestOccupiedRow(occupiedMask(page)) + 1);
    if (height > FUMEN_HEIGHT) {
      throw new SolutionExportError('fumen-height-unsupported');
    }
  }
  try {
    return encodeFastColoredFumenPages(pages);
  } catch (error) {
    if (error instanceof FumenCommentCodecError) {
      throw new SolutionExportError(error.code);
    }
    throw error;
  }
}

export function solutionPageToCtk3Page(page: SolutionExportPage): Ctk3Page {
  validatePage(page);
  return solutionPageToCtk3PageUnchecked(page);
}

function solutionPageToCtk3PageUnchecked(page: SolutionExportPage): Ctk3Page {
  const height = Math.max(1, highestOccupiedRow(occupiedMask(page)) + 1);
  const cells = Array<Ctk3Color>(height * BOARD_WIDTH).fill(null);
  paintCtkMask(cells, page.initialMask, 'G');
  for (const placement of page.placements) {
    paintCtkMask(cells, placement.mask, placement.piece);
  }
  return {
    height,
    cells,
    ...(page.comment ? { comment: page.comment } : {}),
    flags: {
      lock: true,
      mirror: false,
      colorize: true,
      rise: false,
      quiz: false
    }
  };
}

function validateFinesseWitnessEnvelope(witness: FinesseWitnessExport) {
  if (
    !witness ||
    typeof witness.solutionKey !== 'string' ||
    witness.solutionKey.length === 0 ||
    !Number.isSafeInteger(witness.totalInputs) ||
    witness.totalInputs < 0 ||
    !Array.isArray(witness.inputSequence) ||
    witness.inputSequence.length !== witness.totalInputs ||
    !witness.inputSequence.every((input) => FINESSE_INPUTS.has(input)) ||
    !Array.isArray(witness.placements) ||
    witness.placements.length === 0 ||
    witness.placements.length > 60 ||
    witness.inputSequence.filter((input) => input === 'hard-drop').length !==
      witness.placements.length
  ) {
    throw new SolutionExportError('invalid-finesse-witness');
  }
}

function finesseAnnotationInputs(
  value: string | number | undefined,
  fallback: number
): string {
  if (value === undefined) return String(fallback);
  const text = typeof value === 'number' ? String(value) : value;
  if (
    text.length > 64 ||
    !/^(?:0|[1-9]\d*)(?:\.\d+)?$/.test(text) ||
    !Number.isFinite(Number(text)) ||
    Number(text) < 0
  ) {
    throw new SolutionExportError('invalid-finesse-witness');
  }
  return text.includes('.') ? text.replace(/0+$/, '').replace(/\.$/, '') : text;
}

function finesseCtkOperation(
  piece: SolutionPiece,
  declaredRotation: number,
  cells: readonly { x: number; y: number }[]
): Ctk3Operation {
  const rotation = piece === 'O'
    ? 'spawn'
    : piece === 'I' || piece === 'S' || piece === 'Z'
      ? CTK_ROTATIONS[declaredRotation % 2]
      : CTK_ROTATIONS[declaredRotation];
  const target = new Set(cells.map((cell) => `${cell.x},${cell.y}`));
  for (const cell of cells) {
    for (const [offsetX, offsetY] of operationOffsets(piece, rotation)) {
      const candidate: Ctk3Operation = {
        piece,
        rotation,
        x: cell.x - offsetX,
        y: cell.y - offsetY
      };
      const candidateCells = operationCells(candidate);
      if (candidateCells.length === target.size &&
        candidateCells.every(({ x, y }) => target.has(`${x},${y}`))) {
        return candidate;
      }
    }
  }
  throw new SolutionExportError('invalid-finesse-witness');
}

function isSolutionPiece(piece: string): piece is SolutionPiece {
  return piece === 'I' || piece === 'O' || piece === 'T' || piece === 'S' ||
    piece === 'Z' || piece === 'J' || piece === 'L';
}

function emptyColoredRows(height: number): Ctk3Color[][] {
  return Array.from(
    { length: height },
    () => Array<Ctk3Color>(BOARD_WIDTH).fill(null)
  );
}

function paintRowsMask(
  rows: Ctk3Color[][],
  source: bigint,
  color: Exclude<Ctk3Color, null>
) {
  let mask = source;
  while (mask !== 0n) {
    const bit = trailingZeroes(mask);
    const y = Math.floor(bit / BOARD_WIDTH);
    const x = bit % BOARD_WIDTH;
    if (!rows[y] || rows[y][x] !== null) {
      throw new SolutionExportError('invalid-finesse-witness');
    }
    rows[y][x] = color;
    mask &= mask - 1n;
  }
}

function fullColoredRows(rows: readonly Ctk3Color[][]): number[] {
  const result: number[] = [];
  for (let index = 0; index < rows.length; index += 1) {
    if (rows[index].every((cell) => cell !== null)) result.push(index);
  }
  return result;
}

function clearColoredRows(rows: Ctk3Color[][], cleared: readonly number[]) {
  const height = rows.length;
  for (let index = cleared.length - 1; index >= 0; index -= 1) {
    rows.splice(cleared[index], 1);
  }
  while (rows.length < height) rows.push(Array<Ctk3Color>(BOARD_WIDTH).fill(null));
}

function sameColoredRows(left: readonly Ctk3Color[][], right: readonly Ctk3Color[][]) {
  return left.length === right.length && left.every((row, y) =>
    row.length === right[y]?.length && row.every((cell, x) => cell === right[y][x])
  );
}

function paintCtkMask(
  cells: Ctk3Color[],
  mask: bigint,
  value: Exclude<Ctk3Color, null>
) {
  while (mask !== 0n) {
    const bit = trailingZeroes(mask);
    cells[bit] = value;
    mask &= mask - 1n;
  }
}

function validatePage(page: SolutionExportPage) {
  if (!Number.isInteger(page.height) || page.height < 1 || page.height > 24) {
    throw new SolutionExportError('invalid-page');
  }
  const bitLimit = page.height * BOARD_WIDTH;
  if (page.initialMask < 0n || page.initialMask >> BigInt(bitLimit)) {
    throw new SolutionExportError('invalid-page');
  }
  let occupied = page.initialMask;
  for (const placement of page.placements) {
    if (
      placement.mask <= 0n ||
      placement.mask >> BigInt(bitLimit) ||
      popcount(placement.mask) !== 4 ||
      (occupied & placement.mask) !== 0n
    ) {
      throw new SolutionExportError('invalid-page');
    }
    occupied |= placement.mask;
  }
}

function paintDisplayMask(
  cells: SolutionBoardCell[],
  height: number,
  mask: bigint,
  value: Exclude<SolutionBoardCell, null>
) {
  while (mask !== 0n) {
    const bit = trailingZeroes(mask);
    const x = bit % BOARD_WIDTH;
    const y = Math.floor(bit / BOARD_WIDTH);
    if (y >= height) throw new SolutionExportError('invalid-page');
    cells[(height - y - 1) * BOARD_WIDTH + x] = value;
    mask &= mask - 1n;
  }
}

function occupiedMask(page: SolutionExportPage): bigint {
  return page.placements.reduce(
    (mask, placement) => mask | placement.mask,
    page.initialMask
  );
}

function highestOccupiedRow(mask: bigint): number {
  if (mask === 0n) return 0;
  let bit = -1;
  while (mask !== 0n) {
    mask >>= 1n;
    bit += 1;
  }
  return Math.floor(bit / BOARD_WIDTH);
}

function trailingZeroes(value: bigint): number {
  let count = 0;
  while ((value & 1n) === 0n) {
    value >>= 1n;
    count += 1;
  }
  return count;
}

function popcount(value: bigint): number {
  let count = 0;
  while (value !== 0n) {
    value &= value - 1n;
    count += 1;
  }
  return count;
}

function* parsedFumenPages(
  keys: readonly string[]
): Generator<SolutionExportPage> {
  for (const key of keys) {
    const page = parseSolutionKey(key);
    if (!page) throw new SolutionExportError('invalid-solution-key');
    const height = Math.max(1, highestOccupiedRow(occupiedMask(page)) + 1);
    if (height > FUMEN_HEIGHT) {
      throw new SolutionExportError('fumen-height-unsupported');
    }
    yield page;
  }
}
