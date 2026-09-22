import type { ClearraDesktopCliCommandRequest } from '../host/clearraDesktopHost.ts';
import type { WorkspaceLanguage } from '../i18n/languageManifest.ts';
import type { ClearraBoundaryRecoveryPayload, ClearraProductResultPayload } from '../wasm/wasmCommandClient.ts';
import { boardMaskHex, type RuleProfile, type SpinProfile } from './solverWorkspaceModel.ts';
import { cliCommandRequestForDesktop, serializeCliCommandArguments } from './cliCommandModel.ts';
import { validateBoundaryRecoveryPayload } from './boundaryRecoveryPayloadValidation.ts';

export type BoundaryRecoveryRequest = {
  initialBoardMask: bigint;
  targetBoardMask: bigint;
  height: number;
  queue: string;
  stageOneCount: number;
  placements: number;
  maxEarlyPlacements: 0 | 1;
  borrowSourcePosition: number;
  borrowPlacementMask: bigint;
  holdEnabled: boolean;
  rule: RuleProfile;
  spinProfile: SpinProfile;
  preserveB2BStageOne: boolean;
  preserveB2BStageTwo: boolean;
  initialB2B: boolean;
  maxStates: number;
};

export function createBoundaryRecoveryRequest(): BoundaryRecoveryRequest {
  return {
    initialBoardMask: 0n,
    targetBoardMask: 0n,
    height: 8,
    queue: '',
    stageOneCount: 1,
    placements: 2,
    maxEarlyPlacements: 1,
    borrowSourcePosition: 2,
    borrowPlacementMask: 0n,
    holdEnabled: true,
    rule: 'srs-plus',
    spinProfile: 'all-spin-plus',
    preserveB2BStageOne: false,
    preserveB2BStageTwo: false,
    initialB2B: true,
    maxStates: 100_000
  };
}

export function validateBoundaryRecoveryRequest(request: BoundaryRecoveryRequest): string[] {
  const errors: string[] = [];
  const queue = request.queue.trim().toUpperCase();
  if (!/^[IJLOSTZ]{2,14}$/u.test(queue)) errors.push('queue');
  if (!Number.isInteger(request.height) || request.height < 1 || request.height > 25) errors.push('height');
  if (!Number.isInteger(request.stageOneCount) || request.stageOneCount < 1 || request.stageOneCount >= request.placements) errors.push('stage-one');
  if (!Number.isInteger(request.placements) || request.placements > queue.length) errors.push('placements');
  if (request.maxEarlyPlacements !== 0 && request.maxEarlyPlacements !== 1) errors.push('max-early');
  if (request.maxEarlyPlacements === 1 && (!Number.isInteger(request.borrowSourcePosition) || request.borrowSourcePosition <= request.stageOneCount || request.borrowSourcePosition > request.placements)) errors.push('borrow-source');
  if (!Number.isInteger(request.maxStates) || request.maxStates < 1 || request.maxStates > 1_000_000) errors.push('max-states');
  const fieldLimit = 1n << BigInt(Math.max(1, Math.min(25, request.height)) * 10);
  if (request.initialBoardMask < 0n || request.initialBoardMask >= fieldLimit || request.targetBoardMask < 0n || request.targetBoardMask >= fieldLimit) errors.push('board');
  if (request.maxEarlyPlacements === 1 && (request.borrowPlacementMask < 0n || request.borrowPlacementMask >= fieldLimit || bitCount(request.borrowPlacementMask) !== 4)) errors.push('borrow-placement');
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
    '--target-board-mask', boardMaskHex(request.targetBoardMask),
    '--height', String(request.height),
    '--queue', request.queue.trim().toUpperCase(),
    '--stage-one-count', String(request.stageOneCount),
    '--placements', String(request.placements),
    '--max-early-placements', String(request.maxEarlyPlacements),
    '--borrow-source-position', String(request.borrowSourcePosition),
    '--borrow-placement-mask', boardMaskHex(request.borrowPlacementMask),
    request.holdEnabled ? '--hold' : '--no-hold',
    '--rule', request.rule,
    '--spin-profile', request.spinProfile,
    '--initial-b2b', request.initialB2B ? '1' : '0',
    '--max-states', String(request.maxStates)
  ];
  if (request.preserveB2BStageOne) args.push('--preserve-b2b-stage-one');
  if (request.preserveB2BStageTwo) args.push('--preserve-b2b-stage-two');
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
