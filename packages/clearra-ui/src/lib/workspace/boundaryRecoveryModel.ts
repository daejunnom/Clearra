import type { ClearraDesktopCliCommandRequest } from '../host/clearraDesktopHost.ts';
import type { WorkspaceLanguage } from '../i18n/languageManifest.ts';
import type { ClearraBoundaryRecoveryPayload, ClearraProductResultPayload } from '../wasm/wasmCommandClient.ts';
import { boardMaskHex, type RuleProfile, type SpinProfile } from './solverWorkspaceModel.ts';
import { cliCommandRequestForDesktop, serializeCliCommandArguments } from './cliCommandModel.ts';
import { validateBoundaryRecoveryPayload } from './boundaryRecoveryPayloadValidation.ts';

export type BoundaryRecoveryRequest = {
  initialBoardMask: bigint;
  /** First-stage provenance after clears, excluding any early second-stage cells. */
  stageOneBoardMask: bigint;
  targetBoardMask: bigint;
  height: number;
  queue: string;
  queuePattern: string;
  stageOneCount: number;
  /** null delegates terminal count to the exact search. */
  placements: number | null;
  /** Empty keeps occupancy-only search; otherwise one exact lock-time mask per placement role. */
  placementRoleMasks: bigint[];
  maxEarlyPlacements: 0 | 1;
  borrowRolePosition: number;
  borrowPlacementMask: bigint;
  holdEnabled: boolean;
  rule: RuleProfile;
  spinProfile: SpinProfile;
  preserveB2BStageOne: boolean;
  preserveB2BStageTwo: boolean;
  /** One-based bag positions; the stage boundary starts a new bag. */
  preserveB2BBags: number[];
  initialB2B: boolean;
  /** null is unlimited search states, not an unlimited host resource budget. */
  maxStates: number | null;
  maxPatternEvaluations: number;
  maxTotalStates: number | null;
};

export function createBoundaryRecoveryRequest(): BoundaryRecoveryRequest {
  return {
    initialBoardMask: 0n,
    stageOneBoardMask: 0n,
    targetBoardMask: 0n,
    height: 8,
    queue: '',
    queuePattern: '',
    stageOneCount: 1,
    placements: null,
    placementRoleMasks: [],
    maxEarlyPlacements: 1,
    borrowRolePosition: 2,
    borrowPlacementMask: 0n,
    holdEnabled: true,
    rule: 'srs-plus',
    spinProfile: 'all-spin-plus',
    preserveB2BStageOne: false,
    preserveB2BStageTwo: false,
    preserveB2BBags: [],
    initialB2B: true,
    maxStates: null,
    maxPatternEvaluations: 100,
    maxTotalStates: null
  };
}

/** The upper lock horizon is not a guessed terminal count. */
export function recoveryPlacementHorizon(request: BoundaryRecoveryRequest): number {
  return request.placements ?? (request.placementRoleMasks.length || request.queue.trim().length);
}

export function updateRecoveryQueue(request: BoundaryRecoveryRequest, queue: string): BoundaryRecoveryRequest {
  // Geometry belongs to explicit roles, not to the latest queue text length.
  // Keep it editable if a shorter draft queue temporarily makes it invalid.
  return { ...request, queue };
}

function validOptionalStateLimit(value: number | null): boolean {
  return value === null || (Number.isSafeInteger(value) && value >= 1 && value <= 0xffff_ffff);
}

export function boundaryRecoveryBagSlots(stageOneCount: number, placements: number):
    Array<{ position: number; stage: 1 | 2; stageBag: number }> {
  if (!Number.isInteger(stageOneCount) || !Number.isInteger(placements) ||
      stageOneCount < 1 || placements <= stageOneCount || placements > 42) return [];
  const first = Math.ceil(stageOneCount / 7);
  const second = Math.ceil((placements - stageOneCount) / 7);
  return Array.from({ length: first + second }, (_, index) => ({
    position: index + 1,
    stage: index < first ? 1 : 2,
    stageBag: index < first ? index + 1 : index - first + 1
  }));
}

export function validateBoundaryRecoveryRequest(request: BoundaryRecoveryRequest): string[] {
  const errors: string[] = [];
  const queue = request.queue.trim().toUpperCase();
  const horizon = recoveryPlacementHorizon(request);
  if (!/^[IJLOSTZ]{2,42}$/u.test(queue)) errors.push('queue');
  if (!Number.isInteger(request.height) || request.height < 1 || request.height > 25) errors.push('height');
  if (!Number.isInteger(request.stageOneCount) || request.stageOneCount < 1 || request.stageOneCount >= horizon) errors.push('stage-one');
  if ((request.placements !== null && !Number.isInteger(request.placements)) ||
      !Number.isInteger(horizon) || horizon < 2 || horizon > 42 || horizon > queue.length) errors.push('placements');
  if (request.maxEarlyPlacements !== 0 && request.maxEarlyPlacements !== 1) errors.push('max-early');
  if (request.maxEarlyPlacements === 1 && (!Number.isInteger(request.borrowRolePosition) || request.borrowRolePosition <= request.stageOneCount || request.borrowRolePosition > horizon)) errors.push('borrow-role');
  if (!validOptionalStateLimit(request.maxStates)) errors.push('max-states');
  const bagCount = boundaryRecoveryBagSlots(request.stageOneCount, horizon).length;
  if (new Set(request.preserveB2BBags).size !== request.preserveB2BBags.length ||
      request.preserveB2BBags.some((bag) => !Number.isInteger(bag) || bag < 1 || bag > bagCount)) {
    errors.push('b2b-bags');
  }
  if (request.queuePattern.trim()) {
    if (queue.length % 7 !== 0 || request.stageOneCount % 7 !== 0 || horizon !== queue.length ||
        request.placementRoleMasks.length !== queue.length ||
        !Array.from({ length: queue.length / 7 }, (_, bag) => queue.slice(bag * 7, bag * 7 + 7))
          .every((bag) => new Set(bag).size === 7)) errors.push('pattern-roles');
    if (!Number.isInteger(request.maxPatternEvaluations) || request.maxPatternEvaluations < 1 || request.maxPatternEvaluations > 100_000) errors.push('max-pattern-evaluations');
    if (!validOptionalStateLimit(request.maxTotalStates)) errors.push('max-total-states');
  }
  // Invalid numeric drafts remain editable and are reported, never passed to BigInt(NaN).
  const safeHeight = Number.isInteger(request.height) && request.height >= 1 && request.height <= 25 ? request.height : 1;
  const fieldLimit = 1n << BigInt(safeHeight * 10);
  if ([request.initialBoardMask, request.stageOneBoardMask, request.targetBoardMask]
      .some((mask) => mask < 0n || mask >= fieldLimit)) errors.push('board');
  if (request.maxEarlyPlacements === 1 && request.placementRoleMasks.length === 0 &&
      (request.borrowPlacementMask < 0n || request.borrowPlacementMask >= fieldLimit || bitCount(request.borrowPlacementMask) !== 4)) errors.push('borrow-placement');
  if (request.placementRoleMasks.length > 0 &&
      (request.placementRoleMasks.length !== horizon ||
       request.placementRoleMasks.some((mask) => mask < 0n || mask >= fieldLimit || bitCount(mask) !== 4))) {
    errors.push('placement-roles');
  }
  return errors;
}

function bitCount(mask: bigint): number {
  let count = 0;
  for (let remaining = mask; remaining > 0n; remaining &= remaining - 1n) count++;
  return count;
}

export function boundaryRecoveryArguments(request: BoundaryRecoveryRequest): string[] {
  const args = [
    'clearra', 'recovery', 'boundary',
    '--initial-board-mask', boardMaskHex(request.initialBoardMask),
    '--stage-one-board-mask', boardMaskHex(request.stageOneBoardMask),
    '--target-board-mask', boardMaskHex(request.targetBoardMask),
    '--height', String(request.height),
    '--queue', request.queue.trim().toUpperCase(),
    '--stage-one-count', String(request.stageOneCount),
    '--placements', request.placements === null ? 'auto' : String(request.placements),
    '--max-early-placements', String(request.maxEarlyPlacements),
    '--borrow-role-position', String(request.borrowRolePosition),
    request.holdEnabled ? '--hold' : '--no-hold',
    '--rule', request.rule,
    '--spin-profile', request.spinProfile,
    '--initial-b2b', request.initialB2B ? '1' : '0'
  ];
  if (request.maxStates !== null) args.push('--max-states', String(request.maxStates));
  if (request.placementRoleMasks.length === 0) {
    args.push('--borrow-placement-mask', boardMaskHex(request.borrowPlacementMask));
  } else {
    request.placementRoleMasks.forEach((mask, index) => args.push('--role-mask', `${index + 1}:${boardMaskHex(mask)}`));
  }
  if (request.preserveB2BStageOne) args.push('--preserve-b2b-stage-one');
  if (request.preserveB2BStageTwo) args.push('--preserve-b2b-stage-two');
  for (const bag of request.preserveB2BBags) args.push('--preserve-b2b-bag', String(bag));
  if (request.queuePattern.trim()) {
    args.push('--queue-pattern', request.queuePattern.trim());
    args.push('--max-pattern-evaluations', String(request.maxPatternEvaluations));
    if (request.maxTotalStates !== null) args.push('--max-total-states', String(request.maxTotalStates));
  }
  return args;
}

export function boundaryRecoveryCommand(request: BoundaryRecoveryRequest): string {
  return serializeCliCommandArguments(boundaryRecoveryArguments(request));
}

export function boundaryRecoveryDesktopRequest(request: BoundaryRecoveryRequest, language: WorkspaceLanguage): ClearraDesktopCliCommandRequest {
  return cliCommandRequestForDesktop(boundaryRecoveryArguments(request), language);
}

export function boundaryRecoveryPayload(response: { product_result_payload?: ClearraProductResultPayload | null } | null): ClearraBoundaryRecoveryPayload | null {
  const product = response?.product_result_payload;
  return product?.content.payload_kind === 'boundary-recovery' &&
    product.contract === 'boundary-recovery.v1' &&
    product.result_kind === 'boundary-recovery' &&
    validateBoundaryRecoveryPayload(product) === null
    ? product.content.payload
    : null;
}

/** Edit field snapshots independently: clears can move/remove cells between them. */
export type RecoveryField = 'initialBoardMask' | 'stageOneBoardMask' | 'targetBoardMask';

export function updateRecoveryField(request: BoundaryRecoveryRequest, field: RecoveryField, mask: bigint,
    importedHeight = request.height): BoundaryRecoveryRequest {
  const height = Number.isInteger(importedHeight) ? Math.max(request.height, Math.min(24, Math.max(1, importedHeight))) : request.height;
  const limit = (1n << BigInt(height * 10)) - 1n;
  return { ...request, height, [field]: mask & limit };
}

export function updateRecoveryPlacements(request: BoundaryRecoveryRequest, value: number): BoundaryRecoveryRequest {
  if (!Number.isInteger(value) || value < 2 || value > 42) return { ...request, placements: value };
  return { ...request, placements: value,
    preserveB2BBags: value === request.placements ? request.preserveB2BBags : [],
    placementRoleMasks: request.placementRoleMasks.length === 0 ? [] :
      Array.from({ length: value }, (_, index) => request.placementRoleMasks[index] ?? 0n) };
}
