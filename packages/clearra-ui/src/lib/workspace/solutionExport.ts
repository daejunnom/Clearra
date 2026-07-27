import { decoder, encoder, Field } from 'tetris-fumen';

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
};

export type SolutionExportBoard = {
  cells: SolutionBoardCell[];
  height: number;
  page: SolutionExportPage;
};

export type SolutionExportErrorCode =
  | 'invalid-page'
  | 'invalid-solution-key'
  | 'fumen-height-unsupported'
  | 'fumen-roundtrip-mismatch';

export class SolutionExportError extends Error {
  readonly code: SolutionExportErrorCode;

  constructor(code: SolutionExportErrorCode) {
    super(code);
    this.code = code;
  }
}

const BOARD_WIDTH = 10;
const FUMEN_HEIGHT = 23;
const COMPACT_KEY_PATTERN = /^ctk1\|initial=([0-9a-f]{16})\|placements=(.*)$/;
const EXTENDED_KEY_PATTERN =
  /^ctk2\|height=([0-9]{1,2})\|initial=([0-9a-f]{64})\|placements=(.*)$/;
const COMPACT_PLACEMENT_PATTERN = /^([IOTSZJL]):([0-9a-f]{16})$/;
const EXTENDED_PLACEMENT_PATTERN = /^([IOTSZJL]):([0-9a-f]{64})$/;
const COMPACT_MASK_LIMIT = 1n << 64n;

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
  return format === 'fumen' ? encodeColoredFumen(page) : encodeCtk(page);
}

export function encodeColoredFumen(page: SolutionExportPage): string {
  validatePage(page);
  const height = Math.max(1, highestOccupiedRow(occupiedMask(page)) + 1);
  if (height > FUMEN_HEIGHT) {
    throw new SolutionExportError('fumen-height-unsupported');
  }

  const colors = paintColors(page, height);
  const fieldText = Array.from({ length: height }, (_, rowIndex) => {
    const y = height - rowIndex - 1;
    return colors.slice(y * BOARD_WIDTH, (y + 1) * BOARD_WIDTH).join('');
  }).join('');
  const encoded = encoder.encode([{ field: Field.create(fieldText) }]);
  const decoded = decoder.decode(encoded)[0];
  if (!decoded) throw new SolutionExportError('fumen-roundtrip-mismatch');
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < BOARD_WIDTH; x += 1) {
      if (decoded.field.at(x, y) !== colors[y * BOARD_WIDTH + x]) {
        throw new SolutionExportError('fumen-roundtrip-mismatch');
      }
    }
  }
  return encoded;
}

export function encodeCtk(page: SolutionExportPage): string {
  validatePage(page);
  const masks = [page.initialMask, ...page.placements.map((placement) => placement.mask)];
  const compact = masks.every((mask) => mask < COMPACT_MASK_LIMIT);
  const width = compact ? 16 : 64;
  const prefix = compact
    ? `ctk1|initial=${hex(page.initialMask, width)}|placements=`
    : `ctk2|height=${Math.max(
        page.height,
        highestOccupiedRow(occupiedMask(page)) + 1
      )}|initial=${hex(page.initialMask, width)}|placements=`;
  return `${prefix}${page.placements
    .map((placement) => `${placement.piece}:${hex(placement.mask, width)}`)
    .join(',')}`;
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

function paintColors(page: SolutionExportPage, height: number): string[] {
  const colors = Array<string>(height * BOARD_WIDTH).fill('_');
  paintRawMask(colors, page.initialMask, 'X');
  for (const placement of page.placements) {
    paintRawMask(colors, placement.mask, placement.piece);
  }
  return colors;
}

function paintRawMask(cells: string[], mask: bigint, value: string) {
  while (mask !== 0n) {
    const bit = trailingZeroes(mask);
    cells[bit] = value;
    mask &= mask - 1n;
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

function hex(value: bigint, width: number): string {
  return value.toString(16).padStart(width, '0');
}
