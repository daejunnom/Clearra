import {
  boardMaskHex,
  defaultWorkerCount,
  mirrorBoardMask,
  normalizeQueueInput,
  occupiedCellCount,
  parseBrowserQueueInput,
  type RuleProfile,
  type SpinProfile
} from './solverWorkspaceModel';
import { buildDesktopAppRequest, type ClearraDesktopRequest } from '../host/clearraDesktopHost';

export type BuildProbabilityRequest = {
  height: number;
  existingMask: bigint;
  targetMask: bigint;
  queue: string;
  holdEnabled: boolean;
  aggregation: 'buildability' | 'tiling' | 'spin';
  rule: RuleProfile;
  spinProfile: SpinProfile;
  preserveB2B: boolean;
  precomputeBuildDependencies: boolean;
  workers: number;
  useAllLogicalProcessors: boolean;
};

export type BuildProbabilityValidationCode =
  | 'queue_invalid'
  | 'target_lines_invalid'
  | 'build_target_empty'
  | 'build_target_not_tileable'
  | 'build_target_overlap'
  | 'worker_count_invalid';

export function createDefaultBuildProbabilityRequest(): BuildProbabilityRequest {
  return {
    height: 8,
    existingMask: 0n,
    targetMask: 0n,
    queue: '',
    holdEnabled: true,
    aggregation: 'buildability',
    rule: 'srs-plus',
    spinProfile: 't-spins',
    preserveB2B: false,
    precomputeBuildDependencies: false,
    workers: defaultWorkerCount(),
    useAllLogicalProcessors: false
  };
}

export function buildTargetPieceCount(request: BuildProbabilityRequest): number | null {
  const cells = occupiedCellCount(trimBuildProbabilityMask(request.targetMask, request.height));
  return cells > 0 && cells % 4 === 0 ? cells / 4 : null;
}

export function buildProbabilityValidationCodes(
  request: BuildProbabilityRequest
): BuildProbabilityValidationCode[] {
  const errors: BuildProbabilityValidationCode[] = [];
  if (!Number.isInteger(request.height) || request.height < 1 || request.height > 24) {
    errors.push('target_lines_invalid');
  }
  if (!parseBrowserQueueInput(request.queue)) errors.push('queue_invalid');
  const existing = trimBuildProbabilityMask(request.existingMask, request.height);
  const target = trimBuildProbabilityMask(request.targetMask, request.height);
  const targetCellCount = occupiedCellCount(target);
  if (targetCellCount === 0) errors.push('build_target_empty');
  else if (targetCellCount % 4 !== 0) errors.push('build_target_not_tileable');
  if ((existing & target) !== 0n) errors.push('build_target_overlap');
  if (!Number.isInteger(request.workers) || request.workers < 1) {
    errors.push('worker_count_invalid');
  }
  return [...new Set(errors)];
}

export function buildProbabilityCommand(request: BuildProbabilityRequest): string {
  const existing = trimBuildProbabilityMask(request.existingMask, request.height);
  const target = trimBuildProbabilityMask(request.targetMask, request.height);
  const parsedQueue = parseBrowserQueueInput(request.queue);
  const tokens = [
    'clearra',
    'build-probability',
    '--base-mask',
    boardMaskHex(existing),
    '--target-mask',
    boardMaskHex(target),
    '--height',
    String(request.height)
  ];
  if (request.holdEnabled) tokens.push('--hold', 'empty');
  else tokens.push('--no-hold');
  if (request.queue) {
    tokens.push(
      parsedQueue?.kind === 'pattern' ? '--patterns' : '--queue',
      parsedQueue?.source ?? request.queue
    );
  }
  if (request.aggregation === 'tiling') {
    tokens.push('--tiling-only');
  } else {
    tokens.push('--aggregate', request.aggregation);
    tokens.push('--rule', request.rule);
    if (request.aggregation === 'spin' || request.preserveB2B) {
      tokens.push('--spin-profile', request.spinProfile);
    }
    if (request.preserveB2B) tokens.push('--preserve-b2b');
    tokens.push(
      request.precomputeBuildDependencies
        ? '--build-dependency-dag'
        : '--no-build-dependency-dag'
    );
  }
  tokens.push(
    mirrorBoardMask(existing, request.height) === existing ? '--include-mirror' : '--no-mirror'
  );
  tokens.push(
    '--workers',
    String(Math.max(1, Math.trunc(request.workers))),
    '--cpu-warmup'
  );
  if (request.useAllLogicalProcessors) tokens.push('--use-all-cpu-threads');
  return tokens.join(' ');
}

export function buildProbabilityRequestForDesktop(
  request: BuildProbabilityRequest,
  language: 'en' | 'ko'
): ClearraDesktopRequest {
  const existing = trimBuildProbabilityMask(request.existingMask, request.height);
  const target = trimBuildProbabilityMask(request.targetMask, request.height);
  const parsedQueue = parseBrowserQueueInput(request.queue);
  return buildDesktopAppRequest({
    command: 'build-probability',
    language,
    visible_height: request.height,
    base_mask: boardMaskHex(existing),
    target_mask: boardMaskHex(target),
    queue: parsedQueue?.kind === 'fixed' ? parsedQueue.source : '',
    patterns: parsedQueue?.kind === 'pattern' ? parsedQueue.source : '',
    hold_enabled: request.holdEnabled,
    build_aggregation: request.aggregation,
    rule: request.rule,
    spin_profile: request.spinProfile,
    preserve_b2b: request.preserveB2B,
    precompute_build_dependencies: request.precomputeBuildDependencies,
    include_horizontal_mirror: mirrorBoardMask(existing, request.height) === existing,
    workers: 0,
    use_all_logical_processors: request.useAllLogicalProcessors,
    backend: 'cpu',
    allow_backend_fallback: false,
    memory_budget_mb: 0,
    candidate_budget: 0,
    pattern_budget: 0
  });
}

export function trimBuildProbabilityRequest(
  request: BuildProbabilityRequest,
  height: number
): BuildProbabilityRequest {
  const bounded = Math.max(1, Math.min(24, Math.trunc(height || 1)));
  const existingMask = trimBuildProbabilityMask(request.existingMask, bounded);
  return {
    ...request,
    height: bounded,
    existingMask,
    targetMask: trimBuildProbabilityMask(request.targetMask, bounded) & ~existingMask
  };
}

export function trimBuildProbabilityMask(mask: bigint, height: number): bigint {
  const cells = Math.max(0, Math.min(240, Math.trunc(height) * 10));
  if (cells === 0) return 0n;
  return mask & ((1n << BigInt(cells)) - 1n);
}

export function normalizeBuildQueue(value: string): string {
  return normalizeQueueInput(value);
}
