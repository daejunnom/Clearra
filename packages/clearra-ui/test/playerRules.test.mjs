import assert from 'node:assert/strict';
import test from 'node:test';

import {
  PLAYER_BOARD_CELLS,
  PLAYER_BOARD_ROWS,
  PLAYER_BOARD_WIDTH,
  PLAYER_CELL_ID,
  PLAYER_HIDDEN_ROWS,
  PLAYER_KICK_PROFILES,
  PLAYER_SPAWN_Y,
  PLAYER_VISIBLE_ROWS,
  playerCellIdFromCtkColor,
  playerCtkColorFromCellId,
  playerKickCandidates,
  playerPieceOffsets,
  playerPlacementFits,
} from '../src/lib/workspace/player/playerRules.ts';

test('player board and CTK palette contracts are fixed and reversible', () => {
  assert.equal(PLAYER_BOARD_WIDTH, 10);
  assert.equal(PLAYER_BOARD_ROWS, 68);
  assert.equal(PLAYER_BOARD_CELLS, 680);
  assert.equal(PLAYER_VISIBLE_ROWS, 20);
  assert.equal(PLAYER_HIDDEN_ROWS, 48);
  assert.equal(PLAYER_SPAWN_Y, 19);
  assert.deepEqual(PLAYER_CELL_ID, {
    empty: 0,
    G: 1,
    I: 2,
    O: 3,
    T: 4,
    S: 5,
    Z: 6,
    J: 7,
    L: 8,
  });
  for (const color of [null, 'G', 'I', 'O', 'T', 'S', 'Z', 'J', 'L']) {
    assert.equal(playerCtkColorFromCellId(playerCellIdFromCtkColor(color)), color);
  }
});

test('piece shapes use the compact Clearra operation anchors', () => {
  assert.deepEqual(playerPieceOffsets('I', 'spawn'), [
    { x: 0, y: 0 },
    { x: 1, y: 0 },
    { x: 2, y: 0 },
    { x: 3, y: 0 },
  ]);
  assert.deepEqual(playerPieceOffsets('T', 'right'), [
    { x: 0, y: 0 },
    { x: 0, y: 1 },
    { x: 0, y: 2 },
    { x: 1, y: 1 },
  ]);
});

test('SRS+ kick candidates are normalized once and reused without input-time allocation', () => {
  const first = playerKickCandidates('T', 'spawn', 'right');
  const second = playerKickCandidates('T', 'spawn', 'right');
  assert.equal(first, second);
  assert.deepEqual(first[0], { dx: 1, dy: -1 });
  assert.deepEqual(playerKickCandidates('I', 'spawn', 'right')[0], {
    dx: 2,
    dy: -2,
  });
  assert.equal(playerKickCandidates('O', 'spawn', 'reverse').length, 0);
});

test('standard SRS keeps its I-piece quarter-turn table and rejects undefined half turns', () => {
  assert.deepEqual(PLAYER_KICK_PROFILES, ['srs-plus', 'srs', 'srs-x', 'jstris-180']);
  const standard = playerKickCandidates('I', 'spawn', 'right', 'srs');
  const plus = playerKickCandidates('I', 'spawn', 'right', 'srs-plus');
  assert.notDeepEqual(standard, plus);
  assert.deepEqual(standard[1], { dx: 0, dy: -2 });
  assert.equal(playerKickCandidates('T', 'spawn', 'reverse', 'srs').length, 0);
  assert.equal(playerKickCandidates('T', 'spawn', 'reverse', 'srs-plus').length, 6);
  assert.equal(playerKickCandidates('T', 'spawn', 'reverse', 'srs'), playerKickCandidates('T', 'spawn', 'reverse', 'srs'));
});

test('PC solver kick registry is reproduced with profile-exact quarter and half turns', () => {
  const standardIQuarter = playerKickCandidates('I', 'spawn', 'right', 'srs');
  assert.deepEqual(playerKickCandidates('I', 'spawn', 'right', 'srs-x'), standardIQuarter);
  assert.deepEqual(playerKickCandidates('I', 'spawn', 'right', 'jstris-180'), standardIQuarter);

  const plusIHalf = playerKickCandidates('I', 'spawn', 'reverse', 'srs-plus');
  const srsXIHalf = playerKickCandidates('I', 'spawn', 'reverse', 'srs-x');
  const jstrisIHalf = playerKickCandidates('I', 'spawn', 'reverse', 'jstris-180');
  assert.equal(plusIHalf.length, 2);
  assert.equal(srsXIHalf.length, 6);
  assert.deepEqual(jstrisIHalf, plusIHalf);

  assert.equal(playerKickCandidates('T', 'spawn', 'reverse', 'srs-plus').length, 6);
  assert.equal(playerKickCandidates('T', 'spawn', 'reverse', 'srs-x').length, 6);
  assert.equal(playerKickCandidates('T', 'spawn', 'reverse', 'jstris-180').length, 2);
  assert.equal(playerKickCandidates('O', 'spawn', 'right', 'srs-x').length, 1);
  assert.equal(playerKickCandidates('O', 'spawn', 'right', 'jstris-180').length, 0);
});

test('placement collision depends on row occupancy rather than stored colors', () => {
  const empty = new Uint16Array(PLAYER_BOARD_ROWS);
  assert.equal(playerPlacementFits(empty, 'O', 'spawn', 4, 0), true);
  empty[0] = 1 << 4;
  assert.equal(playerPlacementFits(empty, 'O', 'spawn', 4, 0), false);
  assert.equal(playerPlacementFits(empty, 'I', 'spawn', -1, 2), false);
});
