import assert from 'node:assert/strict';
import test from 'node:test';

import { createPlayerGarbageBoard } from '../src/lib/workspace/player/playerGarbage.ts';
import {
  PLAYER_BOARD_CELLS,
  PLAYER_BOARD_ROWS,
  PLAYER_BOARD_WIDTH,
  PLAYER_CELL_ID,
} from '../src/lib/workspace/player/playerRules.ts';

test('seeded garbage is deterministic and every row has one hole and nine CTK G cells', () => {
  const options = { lines: 16, holeSpreadPercent: 37.5, seed: 'garbage-replay' };
  const first = createPlayerGarbageBoard(options);
  const second = createPlayerGarbageBoard(options);

  assert.equal(first.board.length, PLAYER_BOARD_CELLS);
  assert.deepEqual(first.holes, second.holes);
  assert.deepEqual(first.board, second.board);
  assert.equal(first.seed, second.seed);

  for (let row = 0; row < options.lines; row += 1) {
    const cells = first.board.slice(row * PLAYER_BOARD_WIDTH, (row + 1) * PLAYER_BOARD_WIDTH);
    assert.equal(cells.filter((cell) => cell === PLAYER_CELL_ID.empty).length, 1);
    assert.equal(cells.filter((cell) => cell === PLAYER_CELL_ID.G).length, 9);
    assert.equal(cells[first.holes[row]], PLAYER_CELL_ID.empty);
  }
});

test('hole spread extremes have direct, predictable meanings', () => {
  const stacked = createPlayerGarbageBoard({ lines: 40, holeSpreadPercent: 0, seed: 11 });
  assert.equal(new Set(stacked.holes).size, 1);

  const scattered = createPlayerGarbageBoard({ lines: 40, holeSpreadPercent: 100, seed: 11 });
  for (let row = 1; row < scattered.holes.length; row += 1) {
    assert.notEqual(scattered.holes[row], scattered.holes[row - 1]);
  }
});

test('adding garbage raises an existing bottom-up CTK-colored board without mutating it', () => {
  const initialBoard = Array(20).fill(null);
  initialBoard[2] = 'I';
  initialBoard[10 + 7] = 'T';
  const original = initialBoard.slice();

  const result = createPlayerGarbageBoard({
    lines: 3,
    holeSpreadPercent: 0,
    seed: 99,
    initialBoard,
  });

  assert.deepEqual(initialBoard, original);
  assert.equal(result.board[3 * PLAYER_BOARD_WIDTH + 2], PLAYER_CELL_ID.I);
  assert.equal(result.board[4 * PLAYER_BOARD_WIDTH + 7], PLAYER_CELL_ID.T);
  assert.equal(result.board[2], result.holes[0] === 2 ? PLAYER_CELL_ID.empty : PLAYER_CELL_ID.G);
  assert.equal(result.overflowed, false);
  assert.equal(result.discardedCellCount, 0);
});

test('cells raised above the internal board are clipped while in-range cells remain colored', () => {
  const initialBoard = new Uint8Array(PLAYER_BOARD_CELLS);
  initialBoard[(PLAYER_BOARD_ROWS - 3) * PLAYER_BOARD_WIDTH + 1] = PLAYER_CELL_ID.L;
  initialBoard[(PLAYER_BOARD_ROWS - 2) * PLAYER_BOARD_WIDTH + 2] = PLAYER_CELL_ID.S;
  initialBoard[(PLAYER_BOARD_ROWS - 1) * PLAYER_BOARD_WIDTH + 3] = PLAYER_CELL_ID.Z;

  const result = createPlayerGarbageBoard({
    lines: 2,
    holeSpreadPercent: 50,
    seed: 100,
    initialBoard,
  });

  assert.equal(
    result.board[(PLAYER_BOARD_ROWS - 1) * PLAYER_BOARD_WIDTH + 1],
    PLAYER_CELL_ID.L,
  );
  assert.equal(result.board.includes(PLAYER_CELL_ID.S), false);
  assert.equal(result.board.includes(PLAYER_CELL_ID.Z), false);
  assert.equal(result.overflowed, true);
  assert.equal(result.discardedCellCount, 2);
});

test('zero garbage lines preserve the normalized starting field and create no holes', () => {
  const result = createPlayerGarbageBoard({
    lines: 0,
    holeSpreadPercent: 100,
    seed: 7,
    initialBoard: ['G', 'I', 'O', 'T', 'S', 'Z', 'J', 'L', null, null],
  });

  assert.deepEqual(result.holes, []);
  assert.deepEqual(Array.from(result.board.slice(0, 10)), [1, 2, 3, 4, 5, 6, 7, 8, 0, 0]);
  assert.equal(result.board.slice(10).some((cell) => cell !== PLAYER_CELL_ID.empty), false);
});

test('the normalized seed returned from an unseeded result can reproduce it', () => {
  const generated = createPlayerGarbageBoard({ lines: 12, holeSpreadPercent: 65 });
  const replay = createPlayerGarbageBoard({
    lines: 12,
    holeSpreadPercent: 65,
    seed: generated.seed,
  });
  assert.deepEqual(replay.holes, generated.holes);
  assert.deepEqual(replay.board, generated.board);
});

test('garbage inputs and starting fields are validated at their public boundaries', () => {
  for (const lines of [-1, 1.5, PLAYER_BOARD_ROWS + 1]) {
    assert.throws(() => createPlayerGarbageBoard({ lines, holeSpreadPercent: 0 }));
  }
  for (const holeSpreadPercent of [-0.01, 100.01, Number.NaN, Number.POSITIVE_INFINITY]) {
    assert.throws(() => createPlayerGarbageBoard({ lines: 1, holeSpreadPercent }));
  }
  assert.throws(() => createPlayerGarbageBoard({ lines: 1, holeSpreadPercent: 0, seed: Number.NaN }));
  assert.throws(() => createPlayerGarbageBoard({
    lines: 1,
    holeSpreadPercent: 0,
    initialBoard: Array(PLAYER_BOARD_WIDTH + 1).fill(null),
  }));
  assert.throws(() => createPlayerGarbageBoard({
    lines: 1,
    holeSpreadPercent: 0,
    initialBoard: Array(PLAYER_BOARD_CELLS + PLAYER_BOARD_WIDTH).fill(null),
  }));
  assert.throws(() => createPlayerGarbageBoard({
    lines: 1,
    holeSpreadPercent: 0,
    initialBoard: ['not-a-ctk-color', ...Array(9).fill(null)],
  }));
});
