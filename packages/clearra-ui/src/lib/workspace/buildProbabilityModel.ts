import {
  boardMaskHex,
  defaultWorkerCount,
  mirrorBoardMask,
  normalizeQueueInput,
  occupiedCellCount,
  parseBrowserQueueInput,
  type RuleProfile,
  type ScoreProfile,
  type SpinProfile
} from './solverWorkspaceModel.ts';
import type { ClearraDesktopCliCommandRequest } from '../host/clearraDesktopHost.ts';
import {
  buildProbabilityFinesseCommandArguments,
  DEFAULT_BUILD_PROBABILITY_FINESSE,
  DEFAULT_BUILD_PROBABILITY_PATTERN_KNOWLEDGE,
  type BuildProbabilityFinesseMetric,
  type BuildProbabilityPatternKnowledge
} from './buildProbabilityFinesse.ts';
import {
  searchExecutionCommandArguments,
  type SearchExecutionRequest
} from './searchExecutionModel.ts';
import {
  cliCommandRequestForDesktop,
  serializeCliCommandArguments
} from './cliCommandModel.ts';

export type BuildProbabilityRequest = {
  height: number;
  existingMask: bigint;
  targetMask: bigint;
  queue: string;
  holdEnabled: boolean;
  sourcePieces: number | null;
  aggregation: 'buildability' | 'tiling' | 'spin';
  resultMode:
    | 'all-solutions'
    | 'complete-replay-paths'
    | 'minimum-solutions'
    | 'field-average-score'
    | 'fixed-queue-maximum-score'
    | 'highest-score-minimum-set'
    | 'failed-queues';
  failedPatternLimit: number;
  scoreProfile: ScoreProfile;
  initialB2B: number;
  rule: RuleProfile;
  spinProfile: SpinProfile;
  preserveB2B: boolean;
  solutionProbabilities: boolean;
  precomputeBuildDependencies: boolean;
  finesse: BuildProbabilityFinesseMetric;
  patternKnowledge: BuildProbabilityPatternKnowledge;
  workers: number;
  useAllLogicalProcessors: boolean;
};

export type BuildProbabilityValidationCode =
  | 'queue_invalid'
  | 'target_lines_invalid'
  | 'build_target_empty'
  | 'build_target_not_tileable'
  | 'build_target_overlap'
  | 'source_pieces_invalid'
  | 'build_paths_height_invalid'
  | 'fixed_queue_required'
  | 'failed_pattern_limit_invalid'
  | 'initial_b2b_invalid'
  | 'worker_count_invalid';

// The native option is a positive usize. Browser commands execute in wasm32,
// so the shared Web/Desktop surface uses the complete portable native range.
export const BUILD_SOURCE_PIECES_MIN = 1;
export const BUILD_SOURCE_PIECES_MAX = 0xffff_ffff;

export const BUILD_PROBABILITY_PRIMARY_METRIC = Object.freeze({
  id: 'full-future-oracle-build-probability',
  futureVisibility: 'full-future',
  queueKnowledge: 'oracle',
  distinctFrom: 'finesse-pattern-knowledge'
} as const);

export function createDefaultBuildProbabilityRequest(): BuildProbabilityRequest {
  return {
    height: 8,
    existingMask: 0n,
    targetMask: 0n,
    queue: '',
    holdEnabled: true,
    sourcePieces: null,
    aggregation: 'buildability',
    resultMode: 'all-solutions',
    failedPatternLimit: 100,
    scoreProfile: 'tetrio',
    initialB2B: 0,
    rule: 'srs-plus',
    spinProfile: 't-spins',
    preserveB2B: false,
    solutionProbabilities: false,
    precomputeBuildDependencies: false,
    finesse: DEFAULT_BUILD_PROBABILITY_FINESSE,
    patternKnowledge: DEFAULT_BUILD_PROBABILITY_PATTERN_KNOWLEDGE,
    workers: defaultWorkerCount(),
    useAllLogicalProcessors: false
  };
}

/** Preserve inactive user choices; normalization belongs only at execution boundaries. */
export function updateBuildProbabilityDraft(
  request: BuildProbabilityRequest,
  change: Partial<BuildProbabilityRequest>
): BuildProbabilityRequest {
  return { ...request, ...change };
}

export function normalizeBuildProbabilityRequest(
  request: BuildProbabilityRequest
): BuildProbabilityRequest {
  if (request.resultMode !== 'all-solutions') {
    return {
      ...request,
      aggregation: 'buildability',
      spinProfile: 't-spins',
      preserveB2B: false,
      solutionProbabilities: false,
      precomputeBuildDependencies: false,
      finesse: 'off',
      patternKnowledge: 'both'
    };
  }
  if (request.aggregation === 'tiling') {
    return {
      ...request,
      rule: 'srs-plus',
      spinProfile: 't-spins',
      preserveB2B: false,
      solutionProbabilities: false,
      precomputeBuildDependencies: false,
      finesse: 'off',
      patternKnowledge: 'both'
    };
  }
  return {
    ...request,
    spinProfile:
      request.aggregation === 'spin' || request.preserveB2B
        ? request.spinProfile
        : 't-spins',
    patternKnowledge: request.finesse === 'inputs' ? request.patternKnowledge : 'both'
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
  const scoreResultMode = [
    'field-average-score',
    'fixed-queue-maximum-score',
    'highest-score-minimum-set'
  ].includes(request.resultMode);
  if (!Number.isInteger(request.height) || request.height < 1 || request.height > 24) {
    errors.push('target_lines_invalid');
  }
  if (request.queue.trim() !== '' && !parseBrowserQueueInput(request.queue)) {
    errors.push('queue_invalid');
  }
  const existing = trimBuildProbabilityMask(request.existingMask, request.height);
  const target = trimBuildProbabilityMask(request.targetMask, request.height);
  const targetCellCount = occupiedCellCount(target);
  if (targetCellCount === 0) errors.push('build_target_empty');
  else if (targetCellCount % 4 !== 0) errors.push('build_target_not_tileable');
  if ((existing & target) !== 0n) errors.push('build_target_overlap');
  if (
    request.sourcePieces != null &&
    (!Number.isInteger(request.sourcePieces) ||
      request.sourcePieces < BUILD_SOURCE_PIECES_MIN ||
      request.sourcePieces > BUILD_SOURCE_PIECES_MAX)
  ) {
    errors.push('source_pieces_invalid');
  }
  if (request.resultMode === 'complete-replay-paths' && request.height > 6) {
    errors.push('build_paths_height_invalid');
  }
  if (
    request.resultMode === 'fixed-queue-maximum-score' &&
    parseBrowserQueueInput(request.queue)?.kind !== 'fixed'
  ) {
    errors.push('fixed_queue_required');
  }
  if (
    request.resultMode === 'failed-queues' &&
    (!Number.isInteger(request.failedPatternLimit) ||
      request.failedPatternLimit < 1 ||
      request.failedPatternLimit > 1000)
  ) {
    errors.push('failed_pattern_limit_invalid');
  }
  if (
    scoreResultMode &&
    (!Number.isInteger(request.initialB2B) || request.initialB2B < 0 || request.initialB2B > 65_535)
  ) {
    errors.push('initial_b2b_invalid');
  }
  if (!Number.isInteger(request.workers) || request.workers < 1) {
    errors.push('worker_count_invalid');
  }
  return [...new Set(errors)];
}

export function buildProbabilityCommandArguments(request: BuildProbabilityRequest): string[] {
  request = normalizeBuildProbabilityRequest(request);
  const existing = trimBuildProbabilityMask(request.existingMask, request.height);
  const target = trimBuildProbabilityMask(request.targetMask, request.height);
  const parsedQueue = parseBrowserQueueInput(request.queue);
  const tokens = request.resultMode === 'minimum-solutions'
    ? [
        'clearra',
        'build',
        'cover',
        '--base-mask',
        boardMaskHex(existing),
        '--target-mask',
        boardMaskHex(target),
        '--height',
        String(request.height)
      ]
    : [
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
  if (request.sourcePieces != null) {
    tokens.push('--source-pieces', String(request.sourcePieces));
  }
  if (request.queue) {
    tokens.push(
      parsedQueue?.kind === 'pattern' ? '--patterns' : '--queue',
      parsedQueue?.source ?? request.queue
    );
  }
  if (request.resultMode === 'minimum-solutions') {
    tokens.push('--queue-knowledge', 'oracle', '--objective', 'min-cover', '--rule', request.rule);
  } else if (request.aggregation === 'tiling') {
    tokens.push('--tiling-only');
  } else {
    tokens.push('--aggregate', request.aggregation);
    tokens.push('--rule', request.rule);
    if (request.aggregation === 'spin' || request.preserveB2B) {
      tokens.push('--spin-profile', request.spinProfile);
    }
    if (request.preserveB2B) tokens.push('--preserve-b2b');
    if (request.solutionProbabilities) tokens.push('--solution-probabilities');
    tokens.push(
      request.precomputeBuildDependencies
        ? '--build-dependency-dag'
        : '--no-build-dependency-dag'
    );
  }
  if (request.resultMode === 'complete-replay-paths') {
    tokens.push('--result-mode', 'complete-replay-paths');
  } else if (request.resultMode === 'field-average-score') {
    tokens.push(
      '--result-mode',
      'field-average-score',
      '--score-profile',
      request.scoreProfile,
      '--initial-b2b',
      String(request.initialB2B)
    );
  } else if (request.resultMode === 'fixed-queue-maximum-score') {
    tokens.push(
      '--result-mode',
      'fixed-queue-maximum-score',
      '--score-profile',
      request.scoreProfile,
      '--initial-b2b',
      String(request.initialB2B)
    );
  } else if (request.resultMode === 'highest-score-minimum-set') {
    tokens.push(
      '--result-mode',
      'highest-score-minimum-set',
      '--score-profile',
      request.scoreProfile,
      '--initial-b2b',
      String(request.initialB2B)
    );
  } else if (request.resultMode === 'failed-queues') {
    tokens.push(
      '--result-mode',
      'failed-queues',
      '--failed-count',
      String(request.failedPatternLimit)
    );
  }
  if (request.resultMode !== 'minimum-solutions') {
    tokens.push(
      mirrorBoardMask(existing, request.height) === existing ? '--include-mirror' : '--no-mirror'
    );
  }
  tokens.push(...searchExecutionCommandArguments(buildProbabilitySearchExecution(request)));
  tokens.push(...buildProbabilityFinesseCommandArguments(request.finesse, request.patternKnowledge));
  return tokens;
}

export function buildProbabilityCommand(request: BuildProbabilityRequest): string {
  return serializeCliCommandArguments(buildProbabilityCommandArguments(request));
}

export function buildProbabilityRequestForDesktop(
  request: BuildProbabilityRequest,
  language: 'en' | 'ko'
): ClearraDesktopCliCommandRequest {
  return cliCommandRequestForDesktop(buildProbabilityCommandArguments(request), language);
}

function buildProbabilitySearchExecution(
  request: BuildProbabilityRequest
): SearchExecutionRequest {
  return {
    backend: 'cpu',
    gpuDevice: 'auto',
    workers: request.workers,
    useAllLogicalProcessors: request.useAllLogicalProcessors,
    allowBackendFallback: false,
    // Workspace prewarm owns asynchronous worker preparation. A hidden command
    // warmup would make the search wait for every worker instead of dispatching
    // to the first ready worker.
    cpuWarmup: false,
    gpuWarmup: false
  };
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
