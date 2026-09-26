import assert from 'node:assert/strict';
import test from 'node:test';
import { createBoundaryRecoveryRequest, updateRecoveryField, updateRecoveryPlacements,
  boundaryRecoveryArguments, boundaryRecoveryDesktopRequest, validateBoundaryRecoveryRequest }
  from '../src/lib/workspace/boundaryRecoveryModel.ts';

test('recovery snapshots are independent even when line clears move or remove the same cells', () => {
  const original = { ...createBoundaryRecoveryRequest(), queue: 'IO', maxEarlyPlacements: 0,
    height: 4, initialBoardMask: 0x803f0n, stageOneBoardMask: 0x200n, targetBoardMask: 0xe03n };
  assert.deepEqual(validateBoundaryRecoveryRequest(original), []);
  const updated = updateRecoveryField(original, 'stageOneBoardMask', 0xfn);
  assert.equal(updated.initialBoardMask, original.initialBoardMask);
  assert.equal(updated.targetBoardMask, original.targetBoardMask);
  assert.equal(original.stageOneBoardMask, 0x200n);
  assert.equal(updated.stageOneBoardMask, 0xfn);
  const args = boundaryRecoveryArguments(updated);
  assert.equal(args[args.indexOf('--stage-one-board-mask') + 1], '0x000000000000000f');
  assert.deepEqual(boundaryRecoveryDesktopRequest(updated, 'ko').arguments, args);
  assert.equal(createBoundaryRecoveryRequest().stageOneBoardMask, 0n);
});

test('import changes only its selected recovery snapshot and extends bounded editor height', () => {
  const request = { ...createBoundaryRecoveryRequest(), initialBoardMask: 1n, stageOneBoardMask: 2n };
  const updated = updateRecoveryField(request, 'targetBoardMask', 1n << 105n, 12);
  assert.equal(updated.height, 12);
  assert.equal(updated.targetBoardMask, 1n << 105n);
  assert.equal(updated.initialBoardMask, 1n);
  assert.equal(updated.stageOneBoardMask, 2n);
});

test('incomplete numeric drafts are validation failures, not exceptions or unbounded array allocations', () => {
  const request = { ...createBoundaryRecoveryRequest(), queue: 'IO', maxEarlyPlacements: 0 };
  for (const height of [NaN, Infinity, -1, 2.5, 26]) {
    assert.ok(validateBoundaryRecoveryRequest({ ...request, height }).includes('height'));
  }
  for (const placements of [NaN, Infinity, -1, 1, 2.5, 1000000000]) {
    const updated = updateRecoveryPlacements({ ...request, placementRoleMasks: [15n, 60n] }, placements);
    assert.ok(validateBoundaryRecoveryRequest(updated).includes('placements'));
    assert.equal(updated.placementRoleMasks.length, 2);
  }
  assert.ok(validateBoundaryRecoveryRequest({ ...request, height: 4, stageOneBoardMask: 1n << 40n }).includes('board'));
});
