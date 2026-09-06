import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
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

const tetrioSrsXFixture = JSON.parse(readFileSync(
  new URL('../../../tests/fixtures/rules/tetrio_srs_x_standard_tetromino_kicks.json', import.meta.url),
  'utf8',
));

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
  assert.equal(playerKickCandidates('T', 'spawn', 'reverse', 'srs-x').length, 12);
  assert.equal(playerKickCandidates('T', 'spawn', 'reverse', 'jstris-180').length, 2);
  assert.equal(playerKickCandidates('O', 'spawn', 'right', 'srs-x').length, 1);
  assert.equal(playerKickCandidates('O', 'spawn', 'reverse', 'srs-x').length, 1);
  assert.equal(playerKickCandidates('O', 'spawn', 'right', 'jstris-180').length, 0);
});

test('Player SRS-X matches tetrio.js for every standard tetromino transition', () => {
  assert.equal(tetrioSrsXFixture.authority, 'https://tetr.io/js/tetrio.js');
  assert.equal(tetrioSrsXFixture.implicit_origin_attempt, true);
  assert.equal(tetrioSrsXFixture.standard_o.disallow_kick, true);
  assert.equal(tetrioSrsXFixture.standard_o.uses_oo_kicks, false);
  assert.equal(tetrioSrsXFixture.oo_kicks_scope.standard_tetromino, false);

  const transitions = [
    '01', '12', '23', '30', '03', '32', '21', '10', '02', '13', '20', '31',
  ];
  const rotations = ['spawn', 'right', 'reverse', 'left'];
  const jlstzCenters = [[1, 0], [0, 1], [1, 1], [1, 1]];
  const iCenters = [[0, 0], [-2, 2], [0, 1], [-1, 2]];
  let transitionCount = 0;
  let maximumSequenceLength = 0;

  for (const piece of ['I', 'O', 'T', 'S', 'Z', 'J', 'L']) {
    for (const transition of transitions) {
      const fromIndex = Number(transition[0]);
      const toIndex = Number(transition[1]);
      const actual = playerKickCandidates(
        piece,
        rotations[fromIndex],
        rotations[toIndex],
        'srs-x',
      );
      const expected = piece === 'O'
        ? [{ dx: 0, dy: 0 }]
        : normalizedTetrioFixtureOffsets(
          tetrioSrsXFixture.families[piece === 'I' ? 'i' : 'jlstz'][transition],
          piece === 'I' ? iCenters : jlstzCenters,
          fromIndex,
          toIndex,
        );

      assert.deepEqual(actual, expected, `TETR.IO SRS-X ${piece} ${transition}`);
      transitionCount += 1;
      maximumSequenceLength = Math.max(maximumSequenceLength, actual.length);
    }
  }

  assert.equal(transitionCount, tetrioSrsXFixture.expected_standard_transition_count);
  assert.equal(
    maximumSequenceLength,
    tetrioSrsXFixture.expected_max_sequence_length_with_origin,
  );
});

test('placement collision depends on row occupancy rather than stored colors', () => {
  const empty = new Uint16Array(PLAYER_BOARD_ROWS);
  assert.equal(playerPlacementFits(empty, 'O', 'spawn', 4, 0), true);
  empty[0] = 1 << 4;
  assert.equal(playerPlacementFits(empty, 'O', 'spawn', 4, 0), false);
  assert.equal(playerPlacementFits(empty, 'I', 'spawn', -1, 2), false);
});

function normalizedTetrioFixtureOffsets(sourceOffsets, centers, fromIndex, toIndex) {
  const [fromX, fromY] = centers[fromIndex];
  const [toX, toY] = centers[toIndex];
  return [[0, 0], ...sourceOffsets].map(([dx, sourceDy]) => ({
    dx: dx + fromX - toX,
    dy: -sourceDy + fromY - toY,
  }));
}
