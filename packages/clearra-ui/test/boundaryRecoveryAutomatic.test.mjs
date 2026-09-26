import assert from 'node:assert/strict';
import test from 'node:test';
import { createBoundaryRecoveryRequest, recoveryPlacementHorizon, updateRecoveryQueue,
  boundaryRecoveryArguments, boundaryRecoveryDesktopRequest, validateBoundaryRecoveryRequest }
  from '../src/lib/workspace/boundaryRecoveryModel.ts';

function request() {
  return { ...createBoundaryRecoveryRequest(), queue: 'IOT', height: 4,
    stageOneBoardMask: 0xfn, targetBoardMask: 0xc03fn, maxEarlyPlacements: 0 };
}

test('default recovery delegates the lock count and does not insert a hidden state cutoff', () => {
  const input = request();
  assert.equal(input.placements, null);
  assert.equal(input.maxStates, null);
  assert.equal(input.maxTotalStates, null);
  assert.equal(recoveryPlacementHorizon(input), 3); // upper horizon, not 3 mandatory locks
  assert.deepEqual(validateBoundaryRecoveryRequest(input), []);
  const args = boundaryRecoveryArguments(input);
  assert.equal(args[args.indexOf('--placements') + 1], 'auto');
  assert.ok(!args.includes('--max-states'));
  assert.ok(!args.includes('--max-total-states'));
  assert.deepEqual(boundaryRecoveryDesktopRequest(input, 'ko').arguments, args);
});

test('explicit limits and exact roles retain their independent meanings', () => {
  const input = { ...request(), placementRoleMasks: [0xfn, 0xc030n], maxStates: 17 };
  assert.equal(recoveryPlacementHorizon(input), 2);
  assert.deepEqual(validateBoundaryRecoveryRequest(input), []);
  const args = boundaryRecoveryArguments(input);
  assert.equal(args[args.indexOf('--placements') + 1], 'auto');
  assert.equal(args[args.indexOf('--max-states') + 1], '17');
  assert.equal(args.filter(arg => arg === '--role-mask').length, 2);
  const changed = updateRecoveryQueue(input, 'I');
  assert.deepEqual(changed.placementRoleMasks, input.placementRoleMasks);
  assert.ok(validateBoundaryRecoveryRequest(changed).includes('placements'));
  assert.deepEqual(validateBoundaryRecoveryRequest(updateRecoveryQueue(changed, 'IOT')), []);
});

test('no-limit does not turn invalid numeric drafts into automatic requests', () => {
  for (const value of [undefined, NaN, Infinity, 0, -1, 2.5, Number.MAX_SAFE_INTEGER]) {
    assert.ok(validateBoundaryRecoveryRequest({ ...request(), maxStates: value }).includes('max-states'));
  }
  for (const value of [undefined, NaN, Infinity, 0, -1, 2.5, 43]) {
    assert.ok(validateBoundaryRecoveryRequest({ ...request(), placements: value }).includes('placements'));
  }
});

test('pattern help does not grant unsupported reference plans and total states remain opt-in', () => {
  const input = { ...request(), queue: 'IJLOSTZIJLOSTZ', queuePattern: 'P7P7',
    stageOneCount: 7, placementRoleMasks: Array(14).fill(0xfn) };
  assert.deepEqual(validateBoundaryRecoveryRequest(input), []);
  const args = boundaryRecoveryArguments(input);
  assert.ok(!args.includes('--max-total-states'));
  const bounded = boundaryRecoveryArguments({ ...input, maxTotalStates: 31 });
  assert.equal(bounded[bounded.indexOf('--max-total-states')+1], '31');
  assert.ok(validateBoundaryRecoveryRequest({ ...input, placementRoleMasks: [] }).includes('pattern-roles'));
  assert.ok(validateBoundaryRecoveryRequest({ ...input, maxTotalStates: 0 }).includes('max-total-states'));
});
