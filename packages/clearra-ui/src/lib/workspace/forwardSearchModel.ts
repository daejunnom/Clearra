import {
  boardMaskHex,
  normalizeQueueInput,
  parseBrowserQueueInput,
  type RuleProfile,
  type SpinProfile
} from './solverWorkspaceModel.ts';
import type { ClearraDesktopCliCommandRequest } from '../host/clearraDesktopHost.ts';
import { isValidForwardChain, MAX_FORWARD_CHAIN } from './forwardSearchLimits.ts';
import {
  cliCommandRequestForDesktop,
  serializeCliCommandArguments
} from './cliCommandModel.ts';

const MAX_DAMAGE = 0xffff_ffff;
export const MAX_REN_QUEUE_PIECES = 22;
export { MAX_FORWARD_CHAIN } from './forwardSearchLimits.ts';

export type ForwardTool = 'damage' | 'spin-finder' | 'ren';
export type ForwardDamageAggregation = 'maximum' | 'at-least';
export type ForwardSpinLines =
  | 'any'
  | '0'
  | '1'
  | '2'
  | '3'
  | '4'
  | '1+'
  | '2+'
  | '3+'
  | '4+';
export type ForwardSpinCategory = 'any' | 't' | 'other';

export type ForwardSearchRequest = {
  tool: ForwardTool;
  height: number;
  boardMask: bigint;
  queue: string;
  holdEnabled: boolean;
  rule: RuleProfile;
  spinProfile: SpinProfile;
  damageAggregation: ForwardDamageAggregation;
  minimumDamage: number;
  initialCombo: number;
  initialB2B: number;
  preserveB2B: boolean;
  spinLines: ForwardSpinLines;
  spinCategory: ForwardSpinCategory;
  useAllLogicalProcessors: boolean;
};

export type ForwardSearchValidationCode =
  | 'forward_queue_invalid'
  | 'forward_pattern_too_long'
  | 'ren_queue_too_long'
  | 'forward_height_invalid'
  | 'minimum_damage_invalid'
  | 'initial_combo_invalid'
  | 'initial_b2b_invalid';

export function createDefaultForwardSearchRequest(tool: ForwardTool): ForwardSearchRequest {
  return {
    tool,
    height: 8,
    boardMask: 0n,
    queue: '',
    holdEnabled: true,
    rule: 'srs-plus',
    spinProfile: tool === 'damage' ? 'all-mini-plus' : 't-spins',
    damageAggregation: 'maximum',
    minimumDamage: 0,
    initialCombo: 0,
    initialB2B: 0,
    preserveB2B: false,
    spinLines: 'any',
    spinCategory: 'any',
    useAllLogicalProcessors: false
  };
}

export function normalizeForwardQueue(value: string): string {
  return normalizeQueueInput(value);
}

export function forwardSearchValidationCodes(
  request: ForwardSearchRequest
): ForwardSearchValidationCode[] {
  const errors: ForwardSearchValidationCode[] = [];
  const queue = parseBrowserQueueInput(request.queue);
  if (!queue || (request.tool !== 'spin-finder' && queue.kind !== 'fixed')) {
    errors.push('forward_queue_invalid');
  } else if (
    request.tool === 'ren' &&
    queue.sequenceLength > MAX_REN_QUEUE_PIECES
  ) {
    errors.push('ren_queue_too_long');
  } else if (
    request.tool === 'spin-finder' &&
    queue.kind === 'pattern' &&
    queue.sequenceLength > 8
  ) {
    errors.push('forward_pattern_too_long');
  }
  if (!Number.isInteger(request.height) || request.height < 1 || request.height > 24) {
    errors.push('forward_height_invalid');
  }
  if (request.tool !== 'ren' && !isValidForwardChain(request.initialCombo)) {
    errors.push('initial_combo_invalid');
  }
  if (request.tool !== 'ren' && !isValidForwardChain(request.initialB2B)) {
    errors.push('initial_b2b_invalid');
  }
  if (
    request.tool === 'damage' &&
    request.damageAggregation === 'at-least' &&
    (!Number.isInteger(request.minimumDamage) ||
      request.minimumDamage < 0 ||
      request.minimumDamage > MAX_DAMAGE)
  ) {
    errors.push('minimum_damage_invalid');
  }
  return errors;
}

export function buildForwardSearchCommandArguments(
  request: ForwardSearchRequest,
  automaticWorkerLimit?: number
): string[] {
  const queue = parseBrowserQueueInput(request.queue);
  const tokens = [
    'clearra',
    request.tool,
    '--board-mask',
    boardMaskHex(trimForwardBoardMask(request.boardMask, request.height)),
    '--height',
    String(request.height),
    queue?.kind === 'pattern' ? '--patterns' : '--queue',
    queue?.source ?? normalizeForwardQueue(request.queue),
    request.holdEnabled ? '--hold' : '--no-hold',
    '--rule',
    request.rule
  ];
  if (request.tool !== 'ren') tokens.push('--spin-profile', request.spinProfile);
  if (request.tool !== 'ren' && request.preserveB2B) tokens.push('--preserve-b2b');
  if (request.tool === 'damage') {
    if (request.initialCombo > 0) tokens.push('--initial-combo', String(request.initialCombo));
    tokens.push('--initial-b2b', String(request.initialB2B));
    if (request.damageAggregation === 'at-least') {
      tokens.push('--minimum-damage', String(request.minimumDamage));
    }
  } else if (request.tool === 'spin-finder') {
    tokens.push('--lines', request.spinLines);
    tokens.push('--spin-category', request.spinCategory);
  }
  if (automaticWorkerLimit !== undefined) {
    tokens.push('--auto-workers', String(Math.max(1, Math.trunc(automaticWorkerLimit))));
    if (request.useAllLogicalProcessors) tokens.push('--use-all-cpu-threads');
  }
  return tokens;
}

export function buildForwardSearchCommand(
  request: ForwardSearchRequest,
  automaticWorkerLimit?: number
): string {
  return serializeCliCommandArguments(
    buildForwardSearchCommandArguments(request, automaticWorkerLimit)
  );
}

export function forwardSearchRequestForDesktop(
  request: ForwardSearchRequest,
  language: 'en' | 'ko',
  workers: number
): ClearraDesktopCliCommandRequest {
  return cliCommandRequestForDesktop(
    buildForwardSearchCommandArguments(request, workers),
    language
  );
}

export function forwardSourcePieceCount(request: ForwardSearchRequest): number | null {
  return parseBrowserQueueInput(request.queue)?.sequenceLength ?? null;
}

export function trimForwardBoardMask(mask: bigint, height: number): bigint {
  const cells = Math.max(0, Math.min(240, Math.trunc(height) * 10));
  return cells === 0 ? 0n : mask & ((1n << BigInt(cells)) - 1n);
}

export function spinCategoryOptions(spinProfile: SpinProfile): ForwardSpinCategory[] {
  if (
    spinProfile === 'all-spin' ||
    spinProfile === 'all-spin-plus' ||
    spinProfile === 'all-mini' ||
    spinProfile === 'all-mini-plus'
  ) {
    return ['any', 't', 'other'];
  }
  return ['any'];
}
