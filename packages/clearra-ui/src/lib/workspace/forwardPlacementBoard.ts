import type { ClearraForwardPathStep } from '../wasm/wasmCommandClient';
import type {
  SolutionExportPage,
  SolutionExportPlacement
} from './solutionExport';

export type ForwardPiece = 'I' | 'O' | 'T' | 'S' | 'Z' | 'J' | 'L';
export type ForwardBoardCell = ForwardPiece | 'G' | null;
export type ForwardPlacementBoard = {
  cells: ForwardBoardCell[];
  height: number;
  page: SolutionExportPage;
};

const BOARD_WIDTH = 10;

export function replayForwardPlacementBoard(
  initialMask: bigint,
  height: number,
  path: ClearraForwardPathStep[]
): ForwardPlacementBoard | null {
  const logicalRows = emptyRows(height);
  const displayRows = emptyRows(height);
  const logicalToDisplay = Array.from({ length: height }, (_, row) => row);
  const placements: SolutionExportPlacement[] = [];

  if (!writeInitialBoard(logicalRows, displayRows, initialMask)) return null;

  for (const step of path) {
    const piece = forwardPiece(step.piece);
    const placement = parseMask(step.placement_mask);
    if (!piece || placement < 0n || popcount(placement) !== 4) return null;
    const displayPlacement = writePlacement(
      logicalRows,
      displayRows,
      logicalToDisplay,
      placement,
      piece
    );
    if (displayPlacement === null) return null;
    placements.push({ mask: displayPlacement, piece });

    const fullRows = logicalRows
      .map((row, index) => (row.every((cell) => cell !== null) ? index : -1))
      .filter((row) => row >= 0);
    const clearedRowMask = fullRows.reduce((mask, row) => mask | (1 << row), 0) >>> 0;
    if (fullRows.length !== step.cleared_lines || clearedRowMask !== step.cleared_row_mask) {
      return null;
    }

    for (let row = logicalRows.length - 1; row >= 0; row -= 1) {
      if ((clearedRowMask & (1 << row)) === 0) continue;
      logicalRows.splice(row, 1);
      logicalToDisplay.splice(row, 1);
    }
    for (let row = 0; row < fullRows.length; row += 1) {
      logicalRows.push(Array<ForwardBoardCell>(BOARD_WIDTH).fill(null));
      logicalToDisplay.push(displayRows.length);
      displayRows.push(Array<ForwardBoardCell>(BOARD_WIDTH).fill(null));
    }

    if (occupiedMask(logicalRows) !== parseMask(step.board_after_mask)) return null;
  }

  const resultHeight = displayRows.length;
  return {
    cells: displayRows.slice().reverse().flat(),
    height: resultHeight,
    page: { height: resultHeight, initialMask, placements }
  };
}

function emptyRows(height: number): ForwardBoardCell[][] {
  return Array.from({ length: height }, () =>
    Array<ForwardBoardCell>(BOARD_WIDTH).fill(null)
  );
}

function writeInitialBoard(
  logicalRows: ForwardBoardCell[][],
  displayRows: ForwardBoardCell[][],
  mask: bigint
): boolean {
  for (let bit = 0; bit < logicalRows.length * BOARD_WIDTH; bit += 1) {
    if ((mask & (1n << BigInt(bit))) === 0n) continue;
    const y = Math.floor(bit / BOARD_WIDTH);
    const x = bit % BOARD_WIDTH;
    logicalRows[y][x] = 'G';
    displayRows[y][x] = 'G';
  }
  return mask >> BigInt(logicalRows.length * BOARD_WIDTH) === 0n;
}

function writePlacement(
  logicalRows: ForwardBoardCell[][],
  displayRows: ForwardBoardCell[][],
  logicalToDisplay: number[],
  mask: bigint,
  piece: ForwardPiece
): bigint | null {
  if (mask >> BigInt(logicalRows.length * BOARD_WIDTH)) return null;
  let displayMask = 0n;
  for (let bit = 0; bit < logicalRows.length * BOARD_WIDTH; bit += 1) {
    if ((mask & (1n << BigInt(bit))) === 0n) continue;
    const y = Math.floor(bit / BOARD_WIDTH);
    const x = bit % BOARD_WIDTH;
    const displayY = logicalToDisplay[y];
    if (logicalRows[y][x] !== null || displayRows[displayY][x] !== null) return null;
    logicalRows[y][x] = piece;
    displayRows[displayY][x] = piece;
    displayMask |= 1n << BigInt(displayY * BOARD_WIDTH + x);
  }
  return displayMask;
}

function forwardPiece(value: string): ForwardPiece | null {
  return value === 'I' ||
    value === 'O' ||
    value === 'T' ||
    value === 'S' ||
    value === 'Z' ||
    value === 'J' ||
    value === 'L'
    ? value
    : null;
}

function parseMask(value: string): bigint {
  try {
    return BigInt(value);
  } catch {
    return -1n;
  }
}

function popcount(value: bigint): number {
  let count = 0;
  while (value !== 0n) {
    value &= value - 1n;
    count += 1;
  }
  return count;
}

function occupiedMask(rows: ForwardBoardCell[][]): bigint {
  let mask = 0n;
  for (let y = 0; y < rows.length; y += 1) {
    for (let x = 0; x < BOARD_WIDTH; x += 1) {
      if (rows[y][x] !== null) mask |= 1n << BigInt(y * BOARD_WIDTH + x);
    }
  }
  return mask;
}
