import assert from 'node:assert/strict';
import test from 'node:test';
import { availableEarlyPlacementCount, createBoundaryRecoveryRequest, boundaryRecoveryArguments,
  boundaryRecoveryDesktopRequest, updateRecoveryQueue, validateBoundaryRecoveryRequest } from '../src/lib/workspace/boundaryRecoveryModel.ts';
import { validateBoundaryRecoveryPayload } from '../src/lib/workspace/boundaryRecoveryPayloadValidation.ts';

const request = () => ({ ...createBoundaryRecoveryRequest(), queue: 'OOOO', height: 8, stageOneCount: 1 });

test('every available quota is passed unchanged to browser and Desktop with no selected-role requirement', () => {
  assert.equal(availableEarlyPlacementCount(request()), 3);
  for (const count of [0, 1, 2, 3]) {
    const value = { ...request(), maxEarlyPlacements: count };
    assert.deepEqual(validateBoundaryRecoveryRequest(value), []);
    const args = boundaryRecoveryArguments(value);
    assert.equal(args[args.indexOf('--max-early-placements') + 1], String(count));
    assert.ok(!args.includes('--borrow-role-position'));
    assert.ok(!args.includes('--borrow-placement-mask'));
    assert.deepEqual(boundaryRecoveryDesktopRequest(value, 'ko').arguments, args);
  }
});

test('shorter queue drafts preserve the chosen quota and report invalidity instead of silently lowering it', () => {
  const original = { ...request(), maxEarlyPlacements: 3 };
  const short = updateRecoveryQueue(original, 'OO');
  assert.equal(short.maxEarlyPlacements, 3);
  assert.equal(availableEarlyPlacementCount(short), 1);
  assert.ok(validateBoundaryRecoveryRequest(short).includes('max-early'));
  for (const count of [-1, 0.5, 4, Infinity, NaN, undefined, '2']) {
    assert.ok(validateBoundaryRecoveryRequest({ ...request(), maxEarlyPlacements: count }).includes('max-early'));
  }
  assert.ok(validateBoundaryRecoveryRequest({ ...original, borrowRolePosition: 2, borrowPlacementMask: 0xc03n }).includes('borrow-role'));
});

test('one B2B switch serializes the full execution policy without stage or bag filters', () => {
  const value = { ...request(), preserveB2B: true, maxEarlyPlacements: 3 };
  const args = boundaryRecoveryArguments(value);
  assert.ok(args.includes('--preserve-b2b'));
  assert.ok(!args.includes('--preserve-b2b-stage-one'));
  assert.ok(!args.includes('--preserve-b2b-stage-two'));
  assert.ok(!args.includes('--preserve-b2b-bag'));
  assert.ok(!boundaryRecoveryArguments({ ...value, preserveB2B: false }).includes('--preserve-b2b'));
});

test('browser response accepts multi-role evidence but rejects count overflow or a fabricated selected role', () => {
  const report = { status: 'incomplete', knowledge_basis: 'full-fixed-queue', placement_role_scope: 'exact-lock-time',
    max_early_placements: 3, borrow_role_index: null, borrow_placement_mask: null,
    normal_states: 3, recovery_states: 1, stage_one_checkpoint_step: null, checkpoint_is_pc: null,
    borrowed_stage_two_count: 0, steps: [] };
  const wrap = payload => ({ contract: 'boundary-recovery.v1', result_kind: 'boundary-recovery',
    content: { payload_kind: 'boundary-recovery', payload } });
  assert.equal(validateBoundaryRecoveryPayload(wrap(report)), null);
  assert.notEqual(validateBoundaryRecoveryPayload(wrap({ ...report, borrowed_stage_two_count: 4 })), null);
  assert.notEqual(validateBoundaryRecoveryPayload(wrap({ ...report, borrow_role_index: 1, borrow_placement_mask: '0xc03' })), null);
});

test('Auto is a distinct request mode and is never serialized as NaN or a guessed numeric cap', () => {
  for (const queue of ['OO', 'OOOO', 'OOOOO']) {
    const input = { ...request(), queue };
    assert.equal(input.maxEarlyPlacements, 'auto');
    assert.deepEqual(validateBoundaryRecoveryRequest(input), []);
    const args = boundaryRecoveryArguments(input);
    assert.equal(args[args.indexOf('--max-early-placements') + 1], 'auto');
    assert.deepEqual(boundaryRecoveryDesktopRequest(input, 'ko').arguments, args);
    assert.ok(!args.some(argument => argument.includes('NaN')));
  }
});

test('Auto response mode and actual quota survive while inconsistent selected-role evidence is rejected', () => {
  const report = { status: 'incomplete', knowledge_basis: 'full-fixed-queue', placement_role_scope: 'exact-lock-time',
    max_early_placements: 3, early_placement_limit_mode: 'auto', borrow_role_index: null, borrow_placement_mask: null,
    normal_states: 3, recovery_states: 1, stage_one_checkpoint_step: null, checkpoint_is_pc: null,
    borrowed_stage_two_count: 0, steps: [] };
  const wrap = payload => ({ contract: 'boundary-recovery.v1', result_kind: 'boundary-recovery',
    content: { payload_kind: 'boundary-recovery', payload } });
  assert.equal(validateBoundaryRecoveryPayload(wrap(report)), null);
  assert.notEqual(validateBoundaryRecoveryPayload(wrap({ ...report, early_placement_limit_mode: 'guess' })), null);
  assert.notEqual(validateBoundaryRecoveryPayload(wrap({ ...report, max_early_placements: 'auto' })), null);
  assert.notEqual(validateBoundaryRecoveryPayload(wrap({ ...report, max_early_placements: 1,
    borrow_role_index: 1, borrow_placement_mask: '0xc03' })), null);
});

test('a pattern request no longer contains default evaluation or state cutoffs', () => {
  const input = { ...request(), queue: 'IJLOSTZIJLOSTZ', stageOneCount: 7,
    queuePattern: 'P7P7', placementRoleMasks: Array(14).fill(0xfn) };
  assert.equal(input.maxPatternEvaluations, null);
  assert.deepEqual(validateBoundaryRecoveryRequest(input), []);
  const args = boundaryRecoveryArguments(input);
  for (const flag of ['--max-pattern-evaluations', '--max-states', '--max-total-states']) assert.ok(!args.includes(flag));
  assert.deepEqual(boundaryRecoveryDesktopRequest(input, 'en').arguments, args);
});
