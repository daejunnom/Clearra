import assert from 'node:assert/strict';
import test from 'node:test';
import { createBoundaryRecoveryRequest, boundaryRecoveryArguments } from '../src/lib/workspace/boundaryRecoveryModel.ts';
import { isRecoveryEditorHeight, resizeBoundaryRecoveryFields } from '../src/lib/workspace/boundaryRecoveryFieldEditing.ts';

test('Auto and numeric quotas resize without creating a legacy borrow restriction', () => {
  for (const quota of ['auto', 0, 1, 2]) {
    const input = { ...createBoundaryRecoveryRequest(), queue: 'IOT', maxEarlyPlacements: quota };
    const result = resizeBoundaryRecoveryFields(input, 4);
    assert.equal(result.height, 4);
    assert.equal(Object.hasOwn(result, 'borrowPlacementMask'), false);
    assert.equal(Object.hasOwn(result, 'borrowRolePosition'), false);
    const args = boundaryRecoveryArguments(result);
    assert.equal(args[args.indexOf('--height') + 1], '4');
    assert.equal(args[args.indexOf('--max-early-placements') + 1], String(quota));
    assert.ok(!args.includes('--borrow-placement-mask'));
    assert.ok(!args.includes('--borrow-role-position'));
  }
});

test('height changes trim each real mask independently and preserve explicit legacy evidence', () => {
  const high = 1n << 70n;
  const input = { ...createBoundaryRecoveryRequest(), queue: 'IO',
    initialBoardMask: high | 1n, stageOneBoardMask: high | 2n, targetBoardMask: high | 4n,
    placementRoleMasks: [high | 8n, high | 16n], borrowRolePosition: 2,
    borrowPlacementMask: high | 32n, maxEarlyPlacements: 1 };
  const grown = resizeBoundaryRecoveryFields(input, 12);
  assert.deepEqual(grown.placementRoleMasks, input.placementRoleMasks);
  assert.equal(grown.borrowPlacementMask, input.borrowPlacementMask);
  const result = resizeBoundaryRecoveryFields(grown, 4);
  assert.deepEqual([result.initialBoardMask, result.stageOneBoardMask, result.targetBoardMask], [1n, 2n, 4n]);
  assert.deepEqual(result.placementRoleMasks, [8n, 16n]);
  assert.equal(result.borrowPlacementMask, 32n);
  assert.equal(result.borrowRolePosition, 2);
  assert.equal(input.initialBoardMask, high | 1n, 'editing must not mutate the previous request');
});

test('incomplete or invalid height drafts preserve the request and every field', () => {
  const input = { ...createBoundaryRecoveryRequest(), initialBoardMask: 1n << 70n };
  for (const height of [0, NaN, Infinity, -1, 2.5, 25, Number.MAX_SAFE_INTEGER]) {
    assert.equal(isRecoveryEditorHeight(height), false);
    assert.equal(resizeBoundaryRecoveryFields(input, height), input);
  }
  for (const height of [1, 4, 24]) assert.equal(isRecoveryEditorHeight(height), true);
});
