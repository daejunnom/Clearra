import {
  PLAYER_BOARD_CELLS,
  PLAYER_BOARD_ROWS,
  PLAYER_BOARD_WIDTH,
  PLAYER_CELL_ID,
  playerCellIdFromCtkColor,
  type PlayerBoardInputCell,
} from "./playerRules.ts";

export type PlayerGarbageSeed = number | string;

export type PlayerGarbageOptions = Readonly<{
  /** Number of garbage rows inserted at the bottom of the board. */
  lines: number;
  /**
   * Chance, in percent, that a row's hole differs from the row immediately
   * below it. Zero keeps one column; 100 always chooses one of the other nine.
   */
  holeSpreadPercent: number;
  /** Supplying a seed makes both the first hole and every change reproducible. */
  seed?: PlayerGarbageSeed;
  /** Bottom-up, row-major CTK-colored board to raise above the garbage. */
  initialBoard?: ArrayLike<PlayerBoardInputCell>;
}>;

export type PlayerGarbageResult = Readonly<{
  /** Always a complete internal Player board, in bottom-up row-major order. */
  board: Uint8Array;
  /** Hole column for each generated row, ordered from bottom to top. */
  holes: readonly number[];
  /** Normalized seed that can be supplied again to reproduce this result. */
  seed: number;
  /** True when occupied cells were pushed above the internal board and discarded. */
  overflowed: boolean;
  /** Number of occupied starting-field cells discarded above the internal board. */
  discardedCellCount: number;
}>;

/**
 * Builds garbage without mutating the supplied starting field.
 *
 * Player boards are bottom-up. Adding N lines places garbage in rows 0..N-1
 * and raises every existing row by N; cells raised beyond the internal ceiling
 * are deliberately clipped and reported by `overflowed`.
 */
export function createPlayerGarbageBoard(options: PlayerGarbageOptions): PlayerGarbageResult {
  if (!options || typeof options !== "object") {
    throw new TypeError("Player garbage options must be an object.");
  }

  const lines = validateLines(options.lines);
  const holeSpreadPercent = validateHoleSpread(options.holeSpreadPercent);
  const seed = normalizeGarbageSeed(options.seed ?? Date.now());
  const board = new Uint8Array(PLAYER_BOARD_CELLS);

  let discardedCellCount = 0;
  if (options.initialBoard !== undefined) {
    const source = options.initialBoard;
    validateInitialBoard(source);
    for (let index = 0; index < source.length; index += 1) {
      const cell = playerCellIdFromCtkColor(source[index]);
      if (cell === PLAYER_CELL_ID.empty) continue;
      const sourceRow = Math.floor(index / PLAYER_BOARD_WIDTH);
      const targetRow = sourceRow + lines;
      if (targetRow >= PLAYER_BOARD_ROWS) {
        discardedCellCount += 1;
        continue;
      }
      const x = index % PLAYER_BOARD_WIDTH;
      board[targetRow * PLAYER_BOARD_WIDTH + x] = cell;
    }
  }

  const holes: number[] = [];
  if (lines > 0) {
    const random = createSeededRandom(seed);
    let hole = Math.floor(random() * PLAYER_BOARD_WIDTH);
    const changeChance = holeSpreadPercent / 100;

    for (let row = 0; row < lines; row += 1) {
      if (row > 0 && shouldChangeHole(random, changeChance)) {
        hole = chooseDifferentHole(random, hole);
      }
      holes.push(hole);
      const offset = row * PLAYER_BOARD_WIDTH;
      board.fill(PLAYER_CELL_ID.G, offset, offset + PLAYER_BOARD_WIDTH);
      board[offset + hole] = PLAYER_CELL_ID.empty;
    }
  }

  return Object.freeze({
    board,
    holes: Object.freeze(holes),
    seed,
    overflowed: discardedCellCount > 0,
    discardedCellCount,
  });
}

function validateLines(value: number): number {
  if (!Number.isSafeInteger(value)) {
    throw new TypeError("Player garbage lines must be an integer.");
  }
  if (value < 0 || value > PLAYER_BOARD_ROWS) {
    throw new RangeError(`Player garbage lines must be from 0 to ${PLAYER_BOARD_ROWS}.`);
  }
  return value;
}

function validateHoleSpread(value: number): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new TypeError("Player garbage hole spread must be a finite number.");
  }
  if (value < 0 || value > 100) {
    throw new RangeError("Player garbage hole spread must be from 0 to 100 percent.");
  }
  return value;
}

function validateInitialBoard(
  source: ArrayLike<PlayerBoardInputCell>,
): asserts source is ArrayLike<PlayerBoardInputCell> {
  if (!source || typeof source.length !== "number") {
    throw new TypeError("Player garbage starting board must be an array-like value.");
  }
  if (!Number.isSafeInteger(source.length) || source.length < 0 || source.length > PLAYER_BOARD_CELLS) {
    throw new RangeError(`Player garbage starting board must contain at most ${PLAYER_BOARD_CELLS} cells.`);
  }
  if (source.length !== 0 && source.length % PLAYER_BOARD_WIDTH !== 0) {
    throw new RangeError(`Player garbage starting board length must be a multiple of ${PLAYER_BOARD_WIDTH}.`);
  }
}

function normalizeGarbageSeed(seed: PlayerGarbageSeed): number {
  if (typeof seed === "number") {
    if (!Number.isFinite(seed)) throw new TypeError("Player garbage seed must be finite.");
    return (Math.trunc(seed) >>> 0) || 0x6d2b79f5;
  }
  if (typeof seed !== "string") {
    throw new TypeError("Player garbage seed must be a number or string.");
  }
  let hash = 0x811c9dc5;
  for (let index = 0; index < seed.length; index += 1) {
    hash ^= seed.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0) || 0x6d2b79f5;
}

function createSeededRandom(seed: number): () => number {
  let state = seed;
  return () => {
    let value = state >>> 0;
    value ^= value << 13;
    value ^= value >>> 17;
    value ^= value << 5;
    state = value >>> 0 || 0x6d2b79f5;
    return state / 0x1_0000_0000;
  };
}

function shouldChangeHole(random: () => number, chance: number): boolean {
  if (chance <= 0) return false;
  if (chance >= 1) return true;
  return random() < chance;
}

function chooseDifferentHole(random: () => number, previous: number): number {
  const candidate = Math.floor(random() * (PLAYER_BOARD_WIDTH - 1));
  return candidate >= previous ? candidate + 1 : candidate;
}
