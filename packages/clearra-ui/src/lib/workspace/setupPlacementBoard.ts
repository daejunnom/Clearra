import type { ClearraWasmSearchPathStep } from '../wasm/wasmCommandClient';
import type {
  SolutionExportPage,
  SolutionExportPlacement
} from './solutionExport';

export type SetupPiece = 'I' | 'O' | 'T' | 'S' | 'Z' | 'J' | 'L';
export type SetupBoardCell = SetupPiece | 'G' | null;
export type SetupPlacementBoard = {
  cells: SetupBoardCell[];
  height: number;
  page: SolutionExportPage | null;
};

const BOARD_WIDTH = 10;
const SHAPES: Record<SetupPiece, ReadonlyArray<ReadonlyArray<readonly [number, number]>>> = {
  I: [
    [[0, 0], [1, 0], [2, 0], [3, 0]],
    [[0, 0], [0, 1], [0, 2], [0, 3]],
    [[0, 0], [1, 0], [2, 0], [3, 0]],
    [[0, 0], [0, 1], [0, 2], [0, 3]]
  ],
  O: [
    [[0, 0], [1, 0], [0, 1], [1, 1]],
    [[0, 0], [1, 0], [0, 1], [1, 1]],
    [[0, 0], [1, 0], [0, 1], [1, 1]],
    [[0, 0], [1, 0], [0, 1], [1, 1]]
  ],
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

export function replaySetupPlacementBoard(
  finalMask: string,
  path: ClearraWasmSearchPathStep[],
  height = 4
): SetupPlacementBoard | null {
  return replayPlacementBoard(0n, parseMask(finalMask), path, height);
}

export function replaySetupCompletionBoard(
  setupMask: string,
  path: ClearraWasmSearchPathStep[],
  height = 4
): SetupPlacementBoard | null {
  return replayPlacementBoard(parseMask(setupMask), 0n, path, height);
}

function replayPlacementBoard(
  initialMask: bigint,
  expectedFinalMask: bigint,
  path: ClearraWasmSearchPathStep[],
  height: number
): SetupPlacementBoard | null {
  if (initialMask < 0n || expectedFinalMask < 0n) return null;
  const logicalRows = emptyRows(height);
  const displayRows = emptyRows(height);
  const logicalToDisplay = Array.from({ length: height }, (_, row) => row);
  const placements: SolutionExportPlacement[] = [];
  if (!writeInitialBoard(logicalRows, displayRows, initialMask)) return null;

  for (const step of path) {
    const piece = setupPiece(step.piece);
    const shape = piece && SHAPES[piece][step.rotation];
    if (!piece || !shape) return null;

    let placementMask = 0n;
    for (const [dx, dy] of shape) {
      const x = step.x + dx;
      const y = step.y + dy;
      if (x < 0 || x >= BOARD_WIDTH || y < 0 || y >= logicalRows.length) return null;
      const displayY = logicalToDisplay[y];
      if (logicalRows[y][x] !== null || displayRows[displayY][x] !== null) return null;
      logicalRows[y][x] = piece;
      displayRows[displayY][x] = piece;
      placementMask |= 1n << BigInt(displayY * BOARD_WIDTH + x);
    }
    placements.push({ mask: placementMask, piece });

    const fullRows = logicalRows
      .map((row, index) => (row.every((cell) => cell !== null) ? index : -1))
      .filter((row) => row >= 0);
    if (fullRows.length !== step.cleared_lines) return null;

    for (let row = fullRows.length - 1; row >= 0; row -= 1) {
      logicalRows.splice(fullRows[row], 1);
      logicalToDisplay.splice(fullRows[row], 1);
    }
    for (let row = 0; row < fullRows.length; row += 1) {
      logicalRows.push(Array<SetupBoardCell>(BOARD_WIDTH).fill(null));
      logicalToDisplay.push(displayRows.length);
      displayRows.push(Array<SetupBoardCell>(BOARD_WIDTH).fill(null));
    }
  }

  if (occupiedMask(logicalRows) !== expectedFinalMask) return null;
  while (
    displayRows.length > height &&
    displayRows[displayRows.length - 1].every((cell) => cell === null)
  ) {
    displayRows.pop();
  }
  const resultHeight = displayRows.length;
  return {
    cells: displayRows.slice().reverse().flat(),
    height: resultHeight,
    page: { height: resultHeight, initialMask, placements }
  };
}

export function setupFinalBoard(finalMask: string, height = 4): SetupPlacementBoard {
  const mask = parseMask(finalMask);
  const cells = Array<SetupBoardCell>(height * BOARD_WIDTH).fill(null);
  if (mask < 0n) return { cells, height, page: null };
  for (let bit = 0; bit < cells.length; bit += 1) {
    if ((mask & (1n << BigInt(bit))) !== 0n) {
      const x = bit % BOARD_WIDTH;
      const y = Math.floor(bit / BOARD_WIDTH);
      cells[(height - 1 - y) * BOARD_WIDTH + x] = 'G';
    }
  }
  return {
    cells,
    height,
    page: { height, initialMask: mask, placements: [] }
  };
}

function emptyRows(height: number): SetupBoardCell[][] {
  return Array.from({ length: height }, () =>
    Array<SetupBoardCell>(BOARD_WIDTH).fill(null)
  );
}

function writeInitialBoard(
  logicalRows: SetupBoardCell[][],
  displayRows: SetupBoardCell[][],
  mask: bigint
): boolean {
  const cellCount = logicalRows.length * BOARD_WIDTH;
  if (mask >> BigInt(cellCount)) return false;
  for (let bit = 0; bit < cellCount; bit += 1) {
    if ((mask & (1n << BigInt(bit))) === 0n) continue;
    const x = bit % BOARD_WIDTH;
    const y = Math.floor(bit / BOARD_WIDTH);
    logicalRows[y][x] = 'G';
    displayRows[y][x] = 'G';
  }
  return true;
}

function setupPiece(value: string): SetupPiece | null {
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

function occupiedMask(rows: SetupBoardCell[][]): bigint {
  let mask = 0n;
  for (let y = 0; y < rows.length; y += 1) {
    for (let x = 0; x < BOARD_WIDTH; x += 1) {
      if (rows[y][x] !== null) mask |= 1n << BigInt(y * BOARD_WIDTH + x);
    }
  }
  return mask;
}
