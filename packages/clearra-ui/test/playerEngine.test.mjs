import assert from 'node:assert/strict';
import test from 'node:test';

import {
  createPlayerEngine,
  playerBoardIndex,
} from '../src/lib/workspace/player/playerEngine.ts';
import {
  PLAYER_BOARD_CELLS,
  PLAYER_BOARD_ROWS,
  PLAYER_CELL_ID,
  PLAYER_VISIBLE_ROWS,
} from '../src/lib/workspace/player/playerRules.ts';

test('engine exposes a zero-copy render view with a visible spawn and dirty revision', () => {
  const engine = createPlayerEngine({ seed: 7 });
  const view = engine.getRenderView();
  assert.equal(view.board.length, PLAYER_BOARD_CELLS);
  assert.equal(view.rowMasks.length, PLAYER_BOARD_ROWS);
  assert.equal(view.active.y, PLAYER_VISIBLE_ROWS - 1);
  assert.equal(view.active.y < PLAYER_VISIBLE_ROWS, true);
  assert.equal(engine.getRenderView(), view);
  assert.equal(engine.getRenderView().board, view.board);
  const before = engine.revision;
  engine.dispatch('move-left');
  assert.equal(engine.getRenderView(), view);
  assert.equal(engine.revision, before + 1);
});

test('seeded seven-bag order is deterministic and contains every piece once', () => {
  const first = collectPieces(0x1234abcd, 7);
  const second = collectPieces(0x1234abcd, 7);
  assert.deepEqual(first, second);
  assert.deepEqual([...new Set(first)].sort(), ['I', 'J', 'L', 'O', 'S', 'T', 'Z']);
});

test('random reset draws a fresh reproducible seven-bag instead of rewinding the seed', () => {
  const first = createPlayerEngine({ seed: 0x13579bdf, settings: { previewCount: 6 } });
  const second = createPlayerEngine({ seed: 0x13579bdf, settings: { previewCount: 6 } });
  const sequences = [];

  for (let resetIndex = 0; resetIndex < 5; resetIndex += 1) {
    const firstBag = [first.getRenderView().active.piece, ...first.getRenderView().queue];
    const secondBag = [second.getRenderView().active.piece, ...second.getRenderView().queue];
    assert.deepEqual(firstBag, secondBag);
    assert.equal(firstBag.length, 7);
    assert.deepEqual([...new Set(firstBag)].sort(), ['I', 'J', 'L', 'O', 'S', 'T', 'Z']);
    if (sequences.length > 0) assert.notDeepEqual(firstBag, sequences.at(-1));
    sequences.push(firstBag);
    first.reset();
    second.reset();
  }
});

test('an explicit initial queue continues to replay exactly on every reset', () => {
  const selected = ['L', 'J', 'Z', 'S', 'T', 'O', 'I'];
  const engine = createPlayerEngine({
    seed: 0x2468ace0,
    initialQueue: selected,
    settings: { previewCount: 6 },
  });

  for (let resetIndex = 0; resetIndex < 4; resetIndex += 1) {
    assert.deepEqual(
      [engine.getRenderView().active.piece, ...engine.getRenderView().queue],
      selected,
    );
    engine.reset();
  }
});

test('hold is limited until lock and swaps the held piece afterward', () => {
  const engine = createPlayerEngine({ seed: 2, initialQueue: ['T', 'I', 'O', 'S'] });
  const first = engine.getRenderView().active.piece;
  engine.dispatch('hold');
  const afterHold = engine.getRenderView();
  assert.equal(afterHold.hold, first);
  assert.equal(afterHold.active.piece, 'I');
  assert.equal(afterHold.canHold, false);
  assert.equal(engine.dispatch('hold').changed, false);
  engine.dispatch('hard-drop');
  assert.equal(engine.getRenderView().canHold, true);
  engine.dispatch('hold');
  assert.equal(engine.getRenderView().active.piece, first);
});

test('unlimited hold keeps hold available and disabling it restores the used-turn lockout', () => {
  const engine = createPlayerEngine({
    initialQueue: ['T', 'I', 'O', 'S'],
    settings: { gravityG: 0, unlimitedHold: true },
  });

  engine.dispatch('hold');
  assert.deepEqual(
    {
      active: engine.getRenderView().active.piece,
      hold: engine.getRenderView().hold,
      canHold: engine.getRenderView().canHold,
    },
    { active: 'I', hold: 'T', canHold: true },
  );
  engine.dispatch('hold');
  assert.deepEqual(
    {
      active: engine.getRenderView().active.piece,
      hold: engine.getRenderView().hold,
      canHold: engine.getRenderView().canHold,
    },
    { active: 'T', hold: 'I', canHold: true },
  );

  engine.updateSettings({ unlimitedHold: false });
  assert.equal(engine.getRenderView().canHold, false);
  assert.equal(engine.dispatch('hold').changed, false);
  engine.dispatch('hard-drop');
  assert.equal(engine.getRenderView().canHold, true);
});

test('loadBoard copies CTK colors once and derives occupancy masks independently', () => {
  const engine = createPlayerEngine({ seed: 3 });
  const input = ['G', 'I', 'O', 'T', 'S', 'Z', 'J', 'L', null, null];
  engine.loadBoard(input);
  const view = engine.getRenderView();
  assert.deepEqual(Array.from(view.board.slice(0, 10)), [1, 2, 3, 4, 5, 6, 7, 8, 0, 0]);
  assert.equal(view.rowMasks[0], 0xff);
  input[0] = null;
  assert.equal(view.board[0], PLAYER_CELL_ID.G);
});

test('loadQueue validates atomically and reset replays the selected initial queue', () => {
  const engine = createPlayerEngine({ seed: 3, initialQueue: ['T', 'I', 'O'] });
  const replacement = ['Z', 'S', 'L'];

  const loaded = engine.loadQueue(replacement);
  assert.equal(loaded.changed, true);
  assert.equal(engine.getRenderView().active.piece, 'Z');
  assert.deepEqual(engine.getRenderView().queue.slice(0, 2), ['S', 'L']);

  replacement[0] = 'O';
  engine.dispatch('hard-drop');
  assert.equal(engine.getRenderView().active.piece, 'S');
  engine.reset();
  assert.equal(engine.getRenderView().active.piece, 'Z');
  assert.deepEqual(engine.getRenderView().queue.slice(0, 2), ['S', 'L']);

  const beforeInvalid = engine.snapshot();
  assert.throws(() => engine.loadQueue(null), /queue must be an array/i);
  assert.throws(() => engine.loadQueue(['Q']), /unsupported initial queue piece/i);
  assert.equal(engine.revision, beforeInvalid.revision);
  assert.equal(engine.getRenderView().active.piece, beforeInvalid.active.piece);
  assert.deepEqual(engine.getRenderView().queue, beforeInvalid.queue);
});

test('undo respawns the locked piece and redo reapplies the lock without pausing play', () => {
  const engine = createPlayerEngine({
    seed: 0x10203040,
    initialBoard: rowsMissingColumns(1, [3, 4, 5, 6]),
    initialQueue: ['O', 'I', 'T', 'S', 'Z', 'J', 'L'],
    settings: { gravityG: 0, sdf: 41, fixedStepMs: 10, previewCount: 6 },
  });
  engine.dispatch('hold');
  engine.advance(10, {
    left: false,
    right: false,
    softDrop: true,
    horizontalPriority: null,
  });
  const beforeLock = engine.snapshot();
  assert.equal(beforeLock.active.piece, 'I');
  assert.equal(beforeLock.hold, 'O');
  assert.equal(beforeLock.lockElapsedMs, 0);
  assert.ok(beforeLock.score > 0);

  engine.dispatch('hard-drop');
  const afterLock = engine.snapshot();
  assert.equal(engine.getRenderView().canUndo, true);
  assert.equal(engine.getRenderView().canRedo, false);

  const undone = engine.undo();
  assert.equal(undone.changed, true);
  assert.equal(engine.status, 'running');
  assert.equal(engine.getRenderView().canUndo, false);
  assert.equal(engine.getRenderView().canRedo, true);
  const undoView = engine.getRenderView();
  assert.deepEqual(undoView.active, {
    piece: beforeLock.active.piece,
    rotation: 'spawn',
    x: 3,
    y: PLAYER_VISIBLE_ROWS - 1,
  });
  assert.equal(undoView.hold, beforeLock.hold);
  assert.equal(undoView.canHold, true);
  assert.deepEqual(Array.from(undoView.board), Array.from(beforeLock.board));
  assert.deepEqual(undoView.queue, beforeLock.queue);
  assert.equal(undoView.lockElapsedMs, 0);
  assert.equal(undoView.lockResetCount, 0);
  assert.equal(undoView.score, 0);

  engine.updateSettings({ gravityG: 2 });
  const redone = engine.dispatch('redo');
  assert.equal(redone.changed, true);
  assert.equal(redone.locked, true);
  assert.equal(engine.status, 'running');
  assert.equal(engine.settings.gravityG, 2);
  assert.equal(engine.getRenderView().canUndo, true);
  assert.equal(engine.getRenderView().canRedo, false);
  assertHistoryStateEqual(engine.snapshot(), afterLock);
});

test('undo removes the active piece drop score and redo restores it exactly', () => {
  const engine = createPlayerEngine({
    initialQueue: ['O', 'I'],
    settings: { gravityG: 0 },
  });

  engine.dispatch('hard-drop');
  const lockedScore = engine.getRenderView().score;
  assert.equal(lockedScore, 38);

  engine.undo();
  assert.equal(engine.getRenderView().score, 0);
  assert.equal(engine.getRenderView().active.piece, 'O');
  assert.equal(engine.getRenderView().active.y, PLAYER_VISIBLE_ROWS - 1);

  engine.redo();
  assert.equal(engine.getRenderView().score, lockedScore);
  assert.equal(engine.getRenderView().piecesLocked, 1);
});

test('automatic gravity and elapsed time preserve redo until a new piece locks', () => {
  const engine = createPlayerEngine({
    initialQueue: ['O', 'I'],
    settings: {
      gravityG: 1,
      lockDelayMs: 60_000,
      fixedStepMs: 10,
      maxCatchUpSteps: 32,
    },
  });
  engine.dispatch('hard-drop');
  engine.undo();

  const spawnY = engine.getRenderView().active.y;
  engine.advance(100);
  assert.ok(engine.getRenderView().active.y < spawnY);
  assert.equal(engine.getRenderView().canRedo, true);

  assert.equal(engine.redo().changed, true);
  assert.equal(engine.getRenderView().piecesLocked, 1);

  engine.undo();
  engine.advance(10, {
    left: true,
    right: false,
    softDrop: false,
    horizontalPriority: 'left',
  });
  assert.equal(engine.getRenderView().canRedo, false);
});

test('an automatic gravity lock is a new history action and replaces the redo branch', () => {
  const engine = createPlayerEngine({
    initialQueue: ['O', 'I'],
    settings: {
      gravityG: 1000,
      lockDelayMs: 0,
      fixedStepMs: 10,
      maxCatchUpSteps: 32,
    },
  });
  engine.dispatch('hard-drop');
  engine.undo();
  assert.equal(engine.getRenderView().canRedo, true);

  const relocked = engine.advance(10);
  assert.equal(relocked.locked, true);
  assert.equal(engine.getRenderView().canRedo, false);
  assert.equal(engine.redo().changed, false);
});

test('an undone piece respawns at the top and receives the full lock delay', () => {
  const engine = createPlayerEngine({
    initialQueue: ['O', 'I'],
    settings: {
      gravityG: 0.02,
      sdf: 41,
      lockDelayMs: 800,
      lockResetLimit: 2,
      fixedStepMs: 50,
      maxCatchUpSteps: 32,
    },
  });
  engine.advance(50, {
    left: false,
    right: false,
    softDrop: true,
    horizontalPriority: null,
  });
  engine.dispatch('move-left');
  engine.dispatch('move-right');
  assert.equal(engine.getRenderView().lockResetCount, 2);
  engine.dispatch('hard-drop');
  engine.undo();
  assert.equal(engine.getRenderView().lockElapsedMs, 0);
  assert.equal(engine.getRenderView().lockResetCount, 0);
  assert.equal(engine.getRenderView().piecesLocked, 0);
  assert.equal(engine.getRenderView().active.y, PLAYER_VISIBLE_ROWS - 1);
  assert.equal(engine.status, 'running');

  engine.advance(50, {
    left: false,
    right: false,
    softDrop: true,
    horizontalPriority: null,
  });
  engine.advance(700);
  assert.equal(engine.getRenderView().piecesLocked, 0);
  assert.equal(engine.getRenderView().lockElapsedMs, 750);
  const locked = engine.advance(50);
  assert.equal(locked.locked, true);
  assert.equal(engine.getRenderView().piecesLocked, 1);
});

test('resuming from undo creates a branch and reset/load operations clear history', () => {
  const engine = createPlayerEngine({
    seed: 77,
    initialQueue: ['O', 'O', 'O', 'O'],
    settings: { gravityG: 0 },
  });
  engine.dispatch('hard-drop');
  engine.dispatch('hard-drop');
  assert.equal(engine.getRenderView().canUndo, true);

  engine.undo();
  assert.equal(engine.getRenderView().canRedo, true);
  engine.dispatch('move-left');
  assert.equal(engine.getRenderView().canRedo, false);
  assert.equal(engine.redo().changed, false);
  engine.dispatch('hard-drop');
  assert.equal(engine.getRenderView().canUndo, true);

  engine.reset();
  assert.equal(engine.getRenderView().canUndo, false);
  assert.equal(engine.getRenderView().canRedo, false);
  engine.dispatch('hard-drop');
  engine.loadBoard(new Uint8Array(0));
  assert.equal(engine.getRenderView().canUndo, false);
  engine.dispatch('hard-drop');
  engine.loadQueue(['T', 'I']);
  assert.equal(engine.getRenderView().canUndo, false);
  assert.equal(engine.getRenderView().canRedo, false);
});

test('redo restores a lock that topped out as top-out rather than an empty pause', () => {
  const engine = createPlayerEngine({
    initialBoard: fieldWithBlocks([[3, 18], [4, 18], [5, 18]]),
    initialQueue: ['T', 'I'],
    settings: { gravityG: 0 },
  });
  const locked = engine.dispatch('hard-drop');
  assert.equal(locked.topOut, true);
  assert.equal(engine.status, 'top-out');

  engine.undo();
  assert.equal(engine.status, 'running');
  assert.equal(engine.getRenderView().active.piece, 'T');
  engine.redo();
  assert.equal(engine.status, 'top-out');
  assert.equal(engine.getRenderView().active, null);
  assert.equal(engine.getRenderView().canUndo, true);
  assert.equal(engine.getRenderView().canRedo, false);
});

test('lock history is bounded to 128 isolated entries', () => {
  const engine = createPlayerEngine({
    initialQueue: Array(131).fill('O'),
    settings: { gravityG: 0 },
  });
  const placements = [0, 2, 4, 6, 8];
  for (let index = 0; index < 130; index += 1) {
    const targetX = placements[index % placements.length];
    while (engine.getRenderView().active.x > targetX) engine.dispatch('move-left');
    while (engine.getRenderView().active.x < targetX) engine.dispatch('move-right');
    assert.equal(engine.dispatch('hard-drop').locked, true);
  }

  let undoCount = 0;
  while (engine.undo().changed) undoCount += 1;
  assert.equal(undoCount, 128);
  assert.equal(engine.getRenderView().canUndo, false);
  assert.equal(engine.getRenderView().canRedo, true);

  let redoCount = 0;
  while (engine.redo().changed) redoCount += 1;
  assert.equal(redoCount, 128);
  assert.equal(engine.getRenderView().canRedo, false);
});

test('hard drop locks, preserves colors, clears a full row, and spawns the next piece', () => {
  const field = new Uint8Array(20);
  for (let x = 0; x < 10; x += 1) field[x] = x >= 3 && x <= 6 ? 0 : PLAYER_CELL_ID.G;
  const engine = createPlayerEngine({
    seed: 4,
    initialBoard: field,
    initialQueue: ['I', 'T'],
  });
  const result = engine.dispatch('hard-drop');
  assert.equal(result.locked, true);
  assert.equal(result.linesCleared, 1);
  assert.equal(engine.getRenderView().linesCleared, 1);
  assert.equal(engine.getRenderView().piecesLocked, 1);
  assert.equal(engine.getRenderView().active.piece, 'T');
  assert.equal(engine.getRenderView().rowMasks[0], 0);
});

test('a complete imported row is preserved until the first piece locks', () => {
  const field = new Uint8Array(10);
  field.fill(PLAYER_CELL_ID.G);
  const engine = createPlayerEngine({ initialBoard: field, initialQueue: ['I', 'O'] });
  assert.equal(engine.getRenderView().rowMasks[0], 0x3ff);
  assert.equal(engine.getRenderView().linesCleared, 0);
  const result = engine.dispatch('hard-drop');
  assert.equal(result.linesCleared, 1);
  assert.notEqual(engine.getRenderView().rowMasks[0], 0x3ff);
});

test('hidden occupancy alone does not end the game when the next piece can spawn', () => {
  const engine = createPlayerEngine({
    initialBoard: fieldWithBlocks([[0, PLAYER_VISIBLE_ROWS]]),
    initialQueue: ['O', 'I'],
    settings: { gravityG: 0 },
  });

  const locked = engine.dispatch('hard-drop');
  assert.equal(locked.locked, true);
  assert.equal(locked.topOut, false);
  assert.equal(engine.status, 'running');
  assert.equal(engine.getRenderView().active.piece, 'I');
  assert.equal(
    engine.getRenderView().board[playerBoardIndex(0, PLAYER_VISIBLE_ROWS)],
    PLAYER_CELL_ID.G,
  );
});

test('clutch clear spawns above a blocked standard spawn only after a line clear', () => {
  const field = new Uint8Array(PLAYER_BOARD_CELLS);
  for (let y = 0; y < 2; y += 1) {
    for (let x = 0; x < 10; x += 1) {
      if (x !== 4 && x !== 5) field[playerBoardIndex(x, y)] = PLAYER_CELL_ID.G;
    }
  }
  field[playerBoardIndex(3, 21)] = PLAYER_CELL_ID.G;

  const disabled = createPlayerEngine({
    initialBoard: field,
    initialQueue: ['O', 'T'],
    settings: { gravityG: 0, clutchClear: false },
  });
  const disabledLock = disabled.dispatch('hard-drop');
  assert.equal(disabledLock.linesCleared, 2);
  assert.equal(disabledLock.topOut, true);
  assert.equal(disabled.status, 'top-out');

  const enabled = createPlayerEngine({
    initialBoard: field,
    initialQueue: ['O', 'T'],
    settings: { gravityG: 0, clutchClear: true },
  });
  const enabledLock = enabled.dispatch('hard-drop');
  assert.equal(enabledLock.linesCleared, 2);
  assert.equal(enabledLock.topOut, false);
  assert.equal(enabled.status, 'running');
  assert.deepEqual(enabled.getRenderView().active, {
    piece: 'T',
    rotation: 'spawn',
    x: 3,
    y: PLAYER_BOARD_ROWS - 2,
  });

  const noClearField = fieldWithBlocks([[3, PLAYER_VISIBLE_ROWS - 1]]);
  const noClear = createPlayerEngine({
    initialBoard: noClearField,
    initialQueue: ['O', 'T'],
    settings: { gravityG: 0, clutchClear: true },
  });
  const noClearLock = noClear.dispatch('hard-drop');
  assert.equal(noClearLock.linesCleared, 0);
  assert.equal(noClearLock.topOut, true);
  assert.equal(noClear.status, 'top-out');
});

test('undo respawns a previously clutched piece at the highest legal hidden position', () => {
  const field = new Uint8Array(PLAYER_BOARD_CELLS);
  for (let y = 0; y < 2; y += 1) {
    for (let x = 0; x < 10; x += 1) {
      if (x !== 4 && x !== 5) field[playerBoardIndex(x, y)] = PLAYER_CELL_ID.G;
    }
  }
  field[playerBoardIndex(3, 21)] = PLAYER_CELL_ID.G;
  const engine = createPlayerEngine({
    initialBoard: field,
    initialQueue: ['O', 'T', 'I'],
    settings: { gravityG: 0, clutchClear: true },
  });

  assert.equal(engine.dispatch('hard-drop').linesCleared, 2);
  assert.equal(engine.getRenderView().active.y, PLAYER_BOARD_ROWS - 2);
  assert.equal(engine.dispatch('hard-drop').locked, true);

  const undone = engine.undo();
  assert.equal(undone.changed, true);
  assert.equal(undone.topOut, false);
  assert.equal(engine.status, 'running');
  assert.equal(engine.getRenderView().canHold, true);
  assert.deepEqual(engine.getRenderView().active, {
    piece: 'T',
    rotation: 'spawn',
    x: 3,
    y: PLAYER_BOARD_ROWS - 2,
  });
});

test('an explicit NEXT sequence longer than one bag is consumed without truncation', () => {
  const sequence = ['I', 'O', 'T', 'S', 'Z', 'J', 'L', 'L', 'J', 'Z', 'S', 'T'];
  const engine = createPlayerEngine({
    initialQueue: sequence,
    settings: { gravityG: 0, previewCount: 14 },
  });
  const placements = [0, 2, 4, 6, 3];

  for (let index = 0; index < sequence.length; index += 1) {
    assert.equal(engine.getRenderView().active.piece, sequence[index]);
    const targetX = placements[index % placements.length];
    while (engine.getRenderView().active.x > targetX) engine.dispatch('move-left');
    while (engine.getRenderView().active.x < targetX) engine.dispatch('move-right');
    assert.equal(engine.dispatch('hard-drop').locked, true);
    assert.notEqual(engine.status, 'top-out');
  }
});

test('spawn collision reports top-out without writing the active piece', () => {
  const field = new Uint8Array(PLAYER_VISIBLE_ROWS * 10);
  field[playerBoardIndex(3, PLAYER_VISIBLE_ROWS - 1)] = PLAYER_CELL_ID.G;
  const engine = createPlayerEngine({ initialBoard: field, initialQueue: ['T'], seed: 5 });
  assert.equal(engine.status, 'top-out');
  assert.equal(engine.getRenderView().active, null);
  assert.equal(engine.getRenderView().board[playerBoardIndex(3, PLAYER_VISIBLE_ROWS - 1)], 1);
});

test('gravity, lock delay, and reset limit are independent settings', () => {
  const floor = new Uint8Array(10);
  floor.fill(PLAYER_CELL_ID.G);
  const engine = createPlayerEngine({
    initialBoard: floor,
    initialQueue: ['O', 'I'],
    settings: {
      gravityG: 0,
      lockDelayMs: 1000,
      lockResetLimit: 1,
      fixedStepMs: 10,
      maxCatchUpSteps: 4,
      sdf: 41,
    },
  });
  engine.advance(10, { left: false, right: false, softDrop: true, horizontalPriority: null });
  assert.equal(engine.getRenderView().active.y, 1);
  engine.dispatch('move-left');
  assert.equal(engine.getRenderView().lockResetCount, 1);
  engine.dispatch('move-right');
  assert.equal(engine.getRenderView().lockResetCount, 1);

  const instant = createPlayerEngine({
    initialQueue: ['I', 'O'],
    settings: { gravityG: 1000, lockDelayMs: 0, fixedStepMs: 10, maxCatchUpSteps: 2 },
  });
  const result = instant.advance(10);
  assert.equal(result.locked, true);
  assert.equal(instant.getRenderView().piecesLocked, 1);
});

test('successful grounded movement and rotation cannot reset lock delay more than 15 times', () => {
  const floor = new Uint8Array(10);
  floor.fill(PLAYER_CELL_ID.G);
  const engine = createPlayerEngine({
    initialBoard: floor,
    initialQueue: ['O', 'I'],
    settings: {
      gravityG: 0,
      sdf: 41,
      lockDelayMs: 60_000,
      lockResetLimit: 15,
      fixedStepMs: 10,
    },
  });
  engine.advance(10, {
    left: false,
    right: false,
    softDrop: true,
    horizontalPriority: null,
  });

  for (let index = 0; index < 24; index += 1) {
    const action = index % 2 === 0 ? 'move-left' : 'move-right';
    assert.equal(engine.dispatch(action).changed, true);
  }
  assert.equal(engine.getRenderView().lockResetCount, 15);
});

test('the forced-placement default waits 500 ms after contact while gravity is enabled', () => {
  const floor = new Uint8Array(10);
  floor.fill(PLAYER_CELL_ID.G);
  const engine = createPlayerEngine({
    initialBoard: floor,
    initialQueue: ['O', 'I'],
    settings: { gravityG: 0.02, sdf: 41, fixedStepMs: 50, maxCatchUpSteps: 32 },
  });

  engine.advance(50, {
    left: false,
    right: false,
    softDrop: true,
    horizontalPriority: null,
  });
  assert.equal(engine.getRenderView().lockElapsedMs, 50);
  assert.equal(engine.getRenderView().piecesLocked, 0);

  engine.advance(400);
  assert.equal(engine.getRenderView().lockElapsedMs, 450);
  assert.equal(engine.getRenderView().piecesLocked, 0);

  const locked = engine.advance(50);
  assert.equal(locked.locked, true);
  assert.equal(engine.getRenderView().piecesLocked, 1);
});

test('gravity 0 disables forced placement while manual soft drop moves and hard drop locks', () => {
  const normal = createPlayerEngine({
    initialQueue: ['O'],
    settings: {
      gravityG: 0,
      sdf: 2,
      fixedStepMs: 10,
      maxCatchUpSteps: 32,
      lockDelayMs: 40,
    },
  });
  const spawnY = normal.getRenderView().active.y;
  normal.advance(300);
  assert.equal(normal.getRenderView().active.y, spawnY);
  assert.equal(normal.getRenderView().piecesLocked, 0);

  const dropped = normal.advance(300, {
    left: false,
    right: false,
    softDrop: true,
    horizontalPriority: null,
  });
  assert.equal(dropped.locked, false);
  assert.equal(normal.getRenderView().piecesLocked, 0);
  assert.equal(normal.getRenderView().lockElapsedMs, 0);
  normal.advance(10_000);
  assert.equal(normal.getRenderView().piecesLocked, 0);
  assert.equal(normal.getRenderView().lockElapsedMs, 0);
  assert.equal(normal.dispatch('soft-drop').locked, false);
  assert.equal(normal.dispatch('hard-drop').locked, true);
  assert.equal(normal.getRenderView().piecesLocked, 1);
});

test('disabling gravity clears a partial forced-placement timer and re-enabling starts fresh', () => {
  const floor = new Uint8Array(10);
  floor.fill(PLAYER_CELL_ID.G);
  const engine = createPlayerEngine({
    initialBoard: floor,
    initialQueue: ['O', 'I'],
    settings: {
      gravityG: 0.02,
      sdf: 41,
      lockDelayMs: 500,
      fixedStepMs: 50,
      maxCatchUpSteps: 32,
    },
  });

  engine.advance(200, {
    left: false,
    right: false,
    softDrop: true,
    horizontalPriority: null,
  });
  assert.equal(engine.getRenderView().lockElapsedMs, 200);

  engine.updateSettings({ gravityG: 0 });
  engine.advance(50);
  assert.equal(engine.getRenderView().lockElapsedMs, 0);
  engine.advance(5000);
  assert.equal(engine.getRenderView().piecesLocked, 0);

  engine.updateSettings({ gravityG: 0.02 });
  engine.advance(450);
  assert.equal(engine.getRenderView().piecesLocked, 0);
  assert.equal(engine.advance(50).locked, true);
});

test('fixed-step catch-up is bounded and pause blocks simulation', () => {
  const engine = createPlayerEngine({
    settings: { fixedStepMs: 10, maxCatchUpSteps: 2, gravityG: 1 },
    initialQueue: ['I'],
  });
  const beforeY = engine.getRenderView().active.y;
  const beforeTime = engine.getRenderView().elapsedMs;
  const advanced = engine.advance(10_000);
  assert.equal(advanced.steps, 2);
  assert.ok(advanced.droppedMs > 9000);
  assert.equal(engine.getRenderView().active.y, beforeY - 1);
  assert.equal(engine.getRenderView().elapsedMs, beforeTime + 20);
  engine.pause();
  const pausedY = engine.getRenderView().active.y;
  assert.equal(engine.advance(1000).steps, 0);
  assert.equal(engine.getRenderView().active.y, pausedY);
  assert.equal(engine.getRenderView().elapsedMs, beforeTime + 20);
  engine.start();
  assert.equal(engine.status, 'running');
});

test('elapsed time advances without dirtying an otherwise static render view', () => {
  const engine = createPlayerEngine({
    settings: { gravityG: 0, fixedStepMs: 10, maxCatchUpSteps: 20 },
    initialQueue: ['I'],
  });
  const view = engine.getRenderView();
  const beforeRevision = view.revision;
  let notifications = 0;
  const unsubscribe = engine.subscribe(() => {
    notifications += 1;
  }, false);

  const advanced = engine.advance(100);

  assert.equal(advanced.steps, 10);
  assert.equal(advanced.changed, false);
  assert.equal(view.elapsedMs, 100);
  assert.equal(view.revision, beforeRevision);
  assert.equal(notifications, 0);
  unsubscribe();
});

test('zero DAS and ARR move to the wall on the first simulation step', () => {
  const engine = createPlayerEngine({
    initialQueue: ['I'],
    settings: { dasMs: 0, arrMs: 0, gravityG: 0, fixedStepMs: 10 },
  });
  engine.advance(10, {
    left: true,
    right: false,
    softDrop: false,
    horizontalPriority: 'left',
  });
  assert.equal(engine.getRenderView().active.x, 0);
});

test('held zero ARR retries after a wall kick opens horizontal space', () => {
  const engine = createPlayerEngine({
    initialQueue: ['I'],
    settings: { dasMs: 0, arrMs: 0, gravityG: 0, fixedStepMs: 10 },
  });
  const heldLeft = {
    left: true,
    right: false,
    softDrop: false,
    horizontalPriority: 'left',
  };

  engine.advance(10, heldLeft);
  assert.equal(engine.getRenderView().active.x, 0);
  assert.equal(engine.dispatch('rotate-cw').changed, true);
  assert.equal(engine.getRenderView().active.x, 2);

  engine.advance(10, heldLeft);
  assert.equal(engine.getRenderView().active.x, 0);
});

test('the charged default DAS retries zero ARR after a wall kick', () => {
  const engine = createPlayerEngine({
    initialQueue: ['I'],
    settings: {
      dasMs: 83,
      arrMs: 0,
      gravityG: 0,
      fixedStepMs: 10,
      maxCatchUpSteps: 32,
    },
  });
  const heldLeft = {
    left: true,
    right: false,
    softDrop: false,
    horizontalPriority: 'left',
  };

  engine.advance(120, heldLeft);
  assert.equal(engine.getRenderView().active.x, 0);
  assert.equal(engine.dispatch('rotate-cw').changed, true);
  assert.equal(engine.getRenderView().active.x, 2);

  engine.advance(10, heldLeft);
  assert.equal(engine.getRenderView().active.x, 0);
});

test('held zero ARR follows instant soft drop through newly opened horizontal space and locks', () => {
  const field = fieldWithBlocks([[3, PLAYER_VISIBLE_ROWS - 1]]);
  const engine = createPlayerEngine({
    initialBoard: field,
    initialQueue: ['O', 'I'],
    settings: {
      dasMs: 0,
      arrMs: 0,
      gravityG: 0.02,
      sdf: 41,
      lockDelayMs: 50,
      fixedStepMs: 10,
      maxCatchUpSteps: 16,
    },
  });
  const heldLeftAndDown = {
    left: true,
    right: false,
    softDrop: true,
    horizontalPriority: 'left',
  };

  engine.advance(10, heldLeftAndDown);
  assert.deepEqual(
    { x: engine.getRenderView().active.x, y: engine.getRenderView().active.y },
    { x: 4, y: 0 },
  );

  engine.advance(10, heldLeftAndDown);
  assert.equal(engine.getRenderView().active.x, 0);

  const locked = engine.advance(40, heldLeftAndDown);
  assert.equal(locked.locked, true);
  assert.equal(engine.getRenderView().piecesLocked, 1);
});

test('charged default DAS follows instant soft drop into newly opened horizontal space', () => {
  const field = fieldWithBlocks([[3, PLAYER_VISIBLE_ROWS - 1]]);
  const engine = createPlayerEngine({
    initialBoard: field,
    initialQueue: ['O', 'I'],
    settings: {
      dasMs: 83,
      arrMs: 0,
      gravityG: 0.02,
      sdf: 41,
      lockDelayMs: 800,
      fixedStepMs: 10,
      maxCatchUpSteps: 32,
    },
  });
  const heldLeft = {
    left: true,
    right: false,
    softDrop: false,
    horizontalPriority: 'left',
  };

  engine.advance(120, heldLeft);
  assert.deepEqual(
    { x: engine.getRenderView().active.x, y: engine.getRenderView().active.y },
    { x: 4, y: PLAYER_VISIBLE_ROWS - 1 },
  );

  const heldLeftAndDown = { ...heldLeft, softDrop: true };
  engine.advance(10, heldLeftAndDown);
  assert.deepEqual(
    { x: engine.getRenderView().active.x, y: engine.getRenderView().active.y },
    { x: 4, y: 0 },
  );
  engine.advance(10, heldLeftAndDown);
  assert.equal(engine.getRenderView().active.x, 0);

  for (let elapsed = 0; elapsed < 800 && engine.getRenderView().piecesLocked === 0; elapsed += 100) {
    engine.advance(100, heldLeftAndDown);
  }
  assert.equal(engine.getRenderView().piecesLocked, 1);
});

test('reset and resume accept already-held input on their first fixed step', () => {
  const engine = createPlayerEngine({
    initialQueue: ['T', 'I'],
    settings: { dasMs: 0, arrMs: 0, gravityG: 0, fixedStepMs: 10 },
  });
  const heldRight = {
    left: false,
    right: true,
    softDrop: false,
    horizontalPriority: 'right',
  };

  engine.advance(10, heldRight);
  assert.equal(engine.getRenderView().active.x, 7);
  engine.reset();
  assert.equal(engine.getRenderView().active.piece, 'T');
  engine.advance(10, heldRight);
  assert.equal(engine.getRenderView().active.x, 7);

  engine.pause();
  engine.start();
  engine.advance(10, {
    left: true,
    right: false,
    softDrop: false,
    horizontalPriority: 'left',
  });
  assert.equal(engine.getRenderView().active.x, 0);
});

test('kick profile selects SRS+ half turns while standard SRS leaves them unavailable', () => {
  const plus = createPlayerEngine({
    initialQueue: ['I'],
    settings: { gravityG: 0, kickProfile: 'srs-plus' },
  });
  const standard = createPlayerEngine({
    initialQueue: ['I'],
    settings: { gravityG: 0, kickProfile: 'srs' },
  });

  assert.equal(plus.dispatch('rotate-180').changed, true);
  assert.equal(plus.getRenderView().active.rotation, 'reverse');
  assert.equal(standard.dispatch('rotate-180').changed, false);
  assert.equal(standard.getRenderView().active.rotation, 'spawn');
});

test('standard T-spin profile uses exact corners and exposes the scored lock', () => {
  const engine = createPlayerEngine({
    initialBoard: fieldWithBlocks([[3, 18], [5, 18], [3, 20], [5, 20]]),
    initialQueue: ['T'],
    settings: { gravityG: 0, spinProfile: 't-spins' },
  });

  assert.equal(engine.dispatch('rotate-cw').changed, true);
  engine.dispatch('hard-drop');
  const view = engine.getRenderView();
  assert.deepEqual(view.lastSpin, {
    kind: 't-spin',
    piece: 'T',
    mini: false,
    profile: 't-spins',
    rotation: 'right',
    kickIndex: 0,
  });
  assert.equal(view.score, 400);
  assert.equal(view.lastClearInfo.lines, 0);
  assert.equal(view.lastClearInfo.scoreAward, 400);
  assert.deepEqual(engine.snapshot().lastSpin, view.lastSpin);

  const custom = createPlayerEngine({
    initialBoard: fieldWithBlocks([[3, 18], [5, 18], [3, 20], [5, 20]]),
    initialQueue: ['T'],
    settings: {
      gravityG: 0,
      scoreProfile: 'custom',
      scoreModel: { spinScores: [13, 17, 19, 23, 29] },
    },
  });
  custom.dispatch('rotate-cw');
  custom.dispatch('hard-drop');
  assert.equal(custom.getRenderView().score, 13);
});

test('all-spin recognizes immobile non-T rotations and plus adds the immobile T mini fallback', () => {
  const oField = fieldWithBlocks([[4, 18], [3, 19], [6, 19], [4, 21]]);
  const standard = createPlayerEngine({
    initialBoard: oField,
    initialQueue: ['O'],
    settings: { gravityG: 0, spinProfile: 't-spins' },
  });
  standard.dispatch('rotate-cw');
  standard.dispatch('hard-drop');
  assert.equal(standard.getRenderView().lastSpin, null);

  const allSpin = createPlayerEngine({
    initialBoard: oField,
    initialQueue: ['O'],
    settings: { gravityG: 0, spinProfile: 'all-spin' },
  });
  allSpin.dispatch('rotate-cw');
  allSpin.dispatch('hard-drop');
  assert.equal(allSpin.getRenderView().lastSpin.kind, 'all-spin');
  assert.equal(allSpin.getRenderView().lastSpin.piece, 'O');

  const tFallbackField = fieldWithBlocks([[4, 17], [3, 18], [6, 19], [4, 21]]);
  const withoutPlus = createPlayerEngine({
    initialBoard: tFallbackField,
    initialQueue: ['T'],
    settings: { gravityG: 0, spinProfile: 'all-spin' },
  });
  withoutPlus.dispatch('rotate-cw');
  withoutPlus.dispatch('hard-drop');
  assert.equal(withoutPlus.getRenderView().lastSpin, null);

  const plus = createPlayerEngine({
    initialBoard: tFallbackField,
    initialQueue: ['T'],
    settings: { gravityG: 0, spinProfile: 'all-spin-plus' },
  });
  plus.dispatch('rotate-cw');
  plus.dispatch('hard-drop');
  assert.equal(plus.getRenderView().lastSpin.kind, 't-spin-mini');
  assert.equal(plus.getRenderView().lastSpin.mini, true);
  assert.equal(plus.getRenderView().score, 100);
});

test('T-spin plus and all-mini profiles follow their distinct fallback and award contracts', () => {
  const tFallbackField = fieldWithBlocks([[4, 17], [3, 18], [6, 19], [4, 21]]);
  const tPlus = createPlayerEngine({
    initialBoard: tFallbackField,
    initialQueue: ['T'],
    settings: { gravityG: 0, spinProfile: 't-spins-plus' },
  });
  tPlus.dispatch('rotate-cw');
  tPlus.dispatch('hard-drop');
  assert.equal(tPlus.getRenderView().lastSpin.kind, 't-spin-mini');
  assert.equal(tPlus.getRenderView().score, 100);

  const oField = fieldWithBlocks([[4, 18], [3, 19], [6, 19], [4, 21]]);
  for (const profile of ['all-mini', 'all-mini-plus']) {
    const engine = createPlayerEngine({
      initialBoard: oField,
      initialQueue: ['O'],
      settings: { gravityG: 0, spinProfile: profile },
    });
    engine.dispatch('rotate-cw');
    engine.dispatch('hard-drop');
    assert.equal(engine.getRenderView().lastSpin.kind, 'all-spin-mini');
    assert.equal(engine.getRenderView().lastSpin.mini, true);
    assert.equal(engine.getRenderView().score, 100);
  }

  const regularT = createPlayerEngine({
    initialBoard: fieldWithBlocks([[3, 18], [5, 18], [3, 20], [5, 20]]),
    initialQueue: ['T'],
    settings: { gravityG: 0, spinProfile: 'all-mini' },
  });
  regularT.dispatch('rotate-cw');
  regularT.dispatch('hard-drop');
  assert.equal(regularT.getRenderView().lastSpin.kind, 't-spin');
  assert.equal(regularT.getRenderView().score, 400);

  const allMiniPlus = createPlayerEngine({
    initialBoard: tFallbackField,
    initialQueue: ['T'],
    settings: { gravityG: 0, spinProfile: 'all-mini-plus' },
  });
  allMiniPlus.dispatch('rotate-cw');
  allMiniPlus.dispatch('hard-drop');
  assert.equal(allMiniPlus.getRenderView().lastSpin.kind, 't-spin-mini');
});

test('guideline score includes drop and clear points without changing legacy clear counters', () => {
  const field = rowsMissingColumns(2, [3, 4, 5, 6]);
  field.fill(0, 10, 19);
  field[19] = PLAYER_CELL_ID.G;
  const engine = createPlayerEngine({ initialBoard: field, initialQueue: ['I', 'O'] });

  engine.dispatch('hard-drop');
  const view = engine.getRenderView();
  assert.equal(view.score, 138);
  assert.equal(view.lastClear, 1);
  assert.equal(view.lastClearInfo.lines, 1);
  assert.equal(view.lastClearInfo.perfectClear, false);
  assert.equal(view.lastClearInfo.comboIndex, 0);
  assert.equal(view.lastClearInfo.scoreAward, 100);

  const perfectClear = createPlayerEngine({
    initialBoard: rowsMissingColumns(1, [3, 4, 5, 6]),
    initialQueue: ['I', 'O'],
  });
  perfectClear.dispatch('hard-drop');
  assert.equal(perfectClear.getRenderView().score, 938);
  assert.equal(perfectClear.getRenderView().lastClearInfo.perfectClear, true);
  assert.equal(perfectClear.getRenderView().lastClearInfo.scoreAward, 900);
  perfectClear.reset();
  assert.equal(perfectClear.getRenderView().score, 0);
  assert.equal(perfectClear.getRenderView().combo, 0);
  assert.equal(perfectClear.getRenderView().backToBackChain, 0);
  assert.equal(perfectClear.getRenderView().lastSpin, null);
  assert.equal(perfectClear.getRenderView().lastClearInfo, null);
});

test('PC-search score presets preserve their distinct perfect-clear operations', () => {
  const oneLinePerfectClear = rowsMissingColumns(1, [3, 4, 5, 6]);
  const jstris = createPlayerEngine({
    initialBoard: oneLinePerfectClear,
    initialQueue: ['I', 'O'],
    settings: { scoreProfile: 'jstris-ultra' },
  });
  jstris.dispatch('hard-drop');
  assert.equal(jstris.getRenderView().lastClearInfo.scoreAward, 3100);
  assert.equal(jstris.getRenderView().score, 3100);

  const tetrio = createPlayerEngine({
    initialBoard: oneLinePerfectClear,
    initialQueue: ['I', 'O'],
    settings: { scoreProfile: 'tetrio' },
  });
  tetrio.dispatch('hard-drop');
  assert.equal(tetrio.getRenderView().lastClearInfo.scoreAward, 3500);
  assert.equal(tetrio.getRenderView().score, 3538);

  const b2bTetrio = createPlayerEngine({
    initialBoard: rowsMissingColumns(8, [4]),
    initialQueue: ['I', 'I', 'O'],
    settings: { scoreProfile: 'tetrio' },
  });
  for (let index = 0; index < 2; index += 1) {
    b2bTetrio.dispatch('rotate-cw');
    b2bTetrio.dispatch('move-left');
    b2bTetrio.dispatch('hard-drop');
  }
  const b2bView = b2bTetrio.getRenderView();
  assert.equal(b2bView.lastClearInfo.perfectClear, true);
  assert.equal(b2bView.lastClearInfo.backToBackApplied, true);
  assert.equal(b2bView.lastClearInfo.scoreAward, 5300);
});

test('custom coefficients are applied to combo and back-to-back chains', () => {
  const comboEngine = createPlayerEngine({
    initialBoard: rowsMissingColumns(2, [3, 4, 5, 6]),
    initialQueue: ['I', 'I', 'O'],
    settings: { scoreProfile: 'custom', scoreModel: CUSTOM_SCORE_MODEL },
  });
  comboEngine.dispatch('hard-drop');
  comboEngine.dispatch('hard-drop');
  assert.equal(comboEngine.getRenderView().score, 30);
  assert.equal(comboEngine.getRenderView().combo, 2);
  assert.equal(comboEngine.getRenderView().lastClearInfo.comboIndex, 1);

  const b2bEngine = createPlayerEngine({
    initialBoard: rowsMissingColumns(8, [4]),
    initialQueue: ['I', 'I', 'O'],
    settings: { scoreProfile: 'custom', scoreModel: CUSTOM_SCORE_MODEL },
  });
  for (let index = 0; index < 2; index += 1) {
    b2bEngine.dispatch('rotate-cw');
    b2bEngine.dispatch('move-left');
    b2bEngine.dispatch('hard-drop');
  }
  const view = b2bEngine.getRenderView();
  assert.equal(view.score, 310);
  assert.equal(view.backToBackChain, 2);
  assert.equal(view.lastClearInfo.backToBackApplied, true);
  assert.equal(view.lastClearInfo.scoreAward, 210);
  assert.equal(view.lastClearInfo.perfectClear, true);
  b2bEngine.dispatch('hard-drop');
  assert.equal(b2bEngine.getRenderView().combo, 0);
  assert.equal(b2bEngine.getRenderView().backToBackChain, 2);
  assert.equal(b2bEngine.getRenderView().lastClearInfo.lines, 0);
});

test('snapshot copies mutable arrays while render reads retain stable references', () => {
  const engine = createPlayerEngine({ seed: 9 });
  const view = engine.getRenderView();
  const snapshot = engine.snapshot();
  assert.notEqual(snapshot.board, view.board);
  assert.notEqual(snapshot.rowMasks, view.rowMasks);
  snapshot.board[0] = 8;
  assert.equal(view.board[0], 0);
});

function collectPieces(seed, count) {
  const engine = createPlayerEngine({ seed });
  const pieces = [];
  for (let index = 0; index < count; index += 1) {
    pieces.push(engine.getRenderView().active.piece);
    engine.dispatch('hard-drop');
    assert.notEqual(engine.status, 'top-out');
  }
  return pieces;
}

function assertHistoryStateEqual(actual, expected) {
  assert.deepEqual(Array.from(actual.board), Array.from(expected.board));
  assert.deepEqual(Array.from(actual.rowMasks), Array.from(expected.rowMasks));
  assert.deepEqual(actual.active, expected.active);
  assert.equal(actual.ghostY, expected.ghostY);
  assert.equal(actual.hold, expected.hold);
  assert.deepEqual(actual.queue, expected.queue);
  assert.equal(actual.randomState, expected.randomState);
  assert.equal(actual.linesCleared, expected.linesCleared);
  assert.equal(actual.piecesLocked, expected.piecesLocked);
  assert.equal(actual.lastClear, expected.lastClear);
  assert.equal(actual.canHold, expected.canHold);
  assert.equal(actual.lockResetCount, expected.lockResetCount);
  assert.equal(actual.elapsedMs, expected.elapsedMs);
  assert.equal(actual.score, expected.score);
  assert.equal(actual.combo, expected.combo);
  assert.equal(actual.backToBackChain, expected.backToBackChain);
  assert.deepEqual(actual.lastSpin, expected.lastSpin);
  assert.deepEqual(actual.lastClearInfo, expected.lastClearInfo);
}

function fieldWithBlocks(points) {
  const field = new Uint8Array(PLAYER_BOARD_CELLS);
  for (const [x, y] of points) field[playerBoardIndex(x, y)] = PLAYER_CELL_ID.G;
  return field;
}

function rowsMissingColumns(rows, missingColumns) {
  const field = new Uint8Array(rows * 10);
  for (let y = 0; y < rows; y += 1) {
    for (let x = 0; x < 10; x += 1) {
      if (!missingColumns.includes(x)) field[y * 10 + x] = PLAYER_CELL_ID.G;
    }
  }
  return field;
}

const CUSTOM_SCORE_MODEL = Object.freeze({
  lineClearScores: Object.freeze([0, 10, 20, 30, 100]),
  spinScores: Object.freeze([40, 80, 120, 160, 160]),
  miniSpinScores: Object.freeze([10, 20, 40, 80, 80]),
  perfectClearBonuses: Object.freeze([0, 0, 0, 0, 0]),
  backToBackTetrisPerfectClearBonus: 0,
  comboBonusPerStep: 10,
  backToBackMultiplier: 2,
  softDropScorePerCell: 0,
  hardDropScorePerCell: 0,
});
