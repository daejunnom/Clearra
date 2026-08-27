import {
  buildDesktopBuildV2Request,
  type ClearraDesktopBuildV2Request
} from '../host/clearraDesktopHost.ts';
import {
  boardMaskHex,
  defaultWorkerCount,
  occupiedCellCount,
  parseBrowserQueueInput,
  type RuleProfile
} from './solverWorkspaceModel.ts';

export type BuildV2Capability = ClearraDesktopBuildV2Request['capability_id'];
export type BuildV2Objective = ClearraDesktopBuildV2Request['objective'];
export type BuildV2DocumentFormat = 'ctk3' | 'fumen';
export type BuildV2ScoreProfile = 'tetrio' | 'guideline' | 'jstris-ultra';
export type BuildV2SourceKind = 'mask' | 'target-document' | 'solution-document';

export type BuildV2Request = {
  capability: BuildV2Capability;
  height: number;
  baseMask: bigint;
  targetMask: bigint;
  sourcePieceCount: number | null;
  targetFormat: BuildV2DocumentFormat;
  targetDocument: string;
  solutionFormat: BuildV2DocumentFormat;
  solutionDocument: string;
  queue: string;
  holdEnabled: boolean;
  holdPiece: ClearraDesktopBuildV2Request['hold_piece'];
  queueKnowledge: 'oracle' | 'visible-7';
  objective: BuildV2Objective;
  scoreProfile: BuildV2ScoreProfile;
  initialB2B: number;
  rule: RuleProfile;
  workers: number;
  useAllLogicalProcessors: boolean;
};

export type BuildV2ValidationCode =
  | 'queue_invalid'
  | 'target_lines_invalid'
  | 'build_target_empty'
  | 'build_target_not_tileable'
  | 'build_target_overlap'
  | 'source_pieces_invalid'
  | 'target_document_invalid'
  | 'solution_document_invalid'
  | 'objective_invalid'
  | 'initial_b2b_invalid'
  | 'worker_count_invalid';

export const BUILD_V2_CAPABILITIES = Object.freeze([
  'build.cover',
  'build.setup',
  'build.congruent',
  'build.congruent-cover',
  'build.setup-cover',
  'build.setup-cover-percent',
  'build.setup-cover-score',
  'build.evaluate.cover',
  'build.evaluate.minimals',
  'build.evaluate.score',
  'build.evaluate.b2b-cover',
  'build.evaluate.cover-percent'
] as const satisfies readonly BuildV2Capability[]);

const TARGET_DOCUMENT_CAPABILITIES = new Set<BuildV2Capability>([
  'build.setup',
  'build.congruent',
  'build.congruent-cover',
  'build.setup-cover',
  'build.setup-cover-percent',
  'build.setup-cover-score'
]);

const SOLUTION_DOCUMENT_CAPABILITIES = new Set<BuildV2Capability>([
  'build.evaluate.cover',
  'build.evaluate.minimals',
  'build.evaluate.score',
  'build.evaluate.b2b-cover',
  'build.evaluate.cover-percent'
]);

const ALLOWED_OBJECTIVES: Readonly<Record<BuildV2Capability, readonly BuildV2Objective[]>> =
  Object.freeze({
    'build.cover': ['min-cover', 'max-probability-minimum'],
    'build.setup': ['all', 'unique'],
    'build.congruent': ['all', 'unique'],
    'build.congruent-cover': ['min-cover', 'max-probability-minimum'],
    'build.setup-cover': ['min-cover', 'max-probability-minimum'],
    'build.setup-cover-percent': ['all', 'unique'],
    'build.setup-cover-score': ['max-score-cover'],
    'build.evaluate.cover': ['all'],
    'build.evaluate.minimals': ['min-cover'],
    'build.evaluate.score': ['max-score-cover'],
    'build.evaluate.b2b-cover': ['all'],
    'build.evaluate.cover-percent': ['unique']
  });

const DEFAULT_OBJECTIVES: Readonly<Record<BuildV2Capability, BuildV2Objective>> =
  Object.freeze({
    'build.cover': 'min-cover',
    'build.setup': 'unique',
    'build.congruent': 'unique',
    'build.congruent-cover': 'min-cover',
    'build.setup-cover': 'min-cover',
    'build.setup-cover-percent': 'unique',
    'build.setup-cover-score': 'max-score-cover',
    'build.evaluate.cover': 'all',
    'build.evaluate.minimals': 'min-cover',
    'build.evaluate.score': 'max-score-cover',
    'build.evaluate.b2b-cover': 'all',
    'build.evaluate.cover-percent': 'unique'
  });

export function createDefaultBuildV2Request(): BuildV2Request {
  return {
    capability: 'build.setup',
    height: 4,
    baseMask: 0n,
    targetMask: 15n,
    sourcePieceCount: null,
    targetFormat: 'ctk3',
    targetDocument: '',
    solutionFormat: 'ctk3',
    solutionDocument: '',
    queue: 'I',
    holdEnabled: true,
    holdPiece: 'empty',
    queueKnowledge: 'oracle',
    objective: 'unique',
    scoreProfile: 'tetrio',
    initialB2B: 0,
    rule: 'srs-plus',
    workers: defaultWorkerCount(),
    useAllLogicalProcessors: false
  };
}

/** Draft edits retain inactive source/score fields for lossless mode switching. */
export function updateBuildV2Draft(
  request: BuildV2Request,
  change: Partial<BuildV2Request>
): BuildV2Request {
  return { ...request, ...change };
}

export function normalizeBuildV2Request(request: BuildV2Request): BuildV2Request {
  const allowed = buildV2AllowedObjectives(request.capability);
  return {
    ...request,
    objective: allowed.includes(request.objective)
      ? request.objective
      : buildV2DefaultObjective(request.capability)
  };
}

export function buildV2SourceKind(capability: BuildV2Capability): BuildV2SourceKind {
  if (capability === 'build.cover') return 'mask';
  if (TARGET_DOCUMENT_CAPABILITIES.has(capability)) return 'target-document';
  if (SOLUTION_DOCUMENT_CAPABILITIES.has(capability)) return 'solution-document';
  throw new TypeError(`unknown Build v2 capability: ${capability}`);
}

export function buildV2ScoreCapable(capability: BuildV2Capability): boolean {
  return capability === 'build.setup-cover-score' || capability === 'build.evaluate.score';
}

export function buildV2AllowedObjectives(
  capability: BuildV2Capability
): readonly BuildV2Objective[] {
  return ALLOWED_OBJECTIVES[capability];
}

export function buildV2DefaultObjective(capability: BuildV2Capability): BuildV2Objective {
  return DEFAULT_OBJECTIVES[capability];
}

export function buildV2ValidationCodes(request: BuildV2Request): BuildV2ValidationCode[] {
  const errors: BuildV2ValidationCode[] = [];
  const parsedQueue = parseBrowserQueueInput(request.queue);
  if (!request.queue.trim() || !parsedQueue) errors.push('queue_invalid');
  if (!buildV2AllowedObjectives(request.capability).includes(request.objective)) {
    errors.push('objective_invalid');
  }
  const source = buildV2SourceKind(request.capability);
  if (source === 'mask') {
    if (!Number.isInteger(request.height) || request.height < 1 || request.height > 24) {
      errors.push('target_lines_invalid');
    }
    const base = trimBuildV2Mask(request.baseMask, request.height);
    const target = trimBuildV2Mask(request.targetMask, request.height);
    const cells = occupiedCellCount(target);
    if (cells === 0) errors.push('build_target_empty');
    else if (cells % 4 !== 0) errors.push('build_target_not_tileable');
    if ((base & target) !== 0n) errors.push('build_target_overlap');
    if (
      request.sourcePieceCount !== null &&
      (!Number.isInteger(request.sourcePieceCount) ||
        request.sourcePieceCount < 1 ||
        request.sourcePieceCount > 0xffff_ffff)
    ) {
      errors.push('source_pieces_invalid');
    }
  } else if (
    source === 'target-document' &&
    !validBuildV2Document(request.targetFormat, request.targetDocument)
  ) {
    errors.push('target_document_invalid');
  } else if (
    source === 'solution-document' &&
    !validBuildV2Document(request.solutionFormat, request.solutionDocument)
  ) {
    errors.push('solution_document_invalid');
  }
  if (
    buildV2ScoreCapable(request.capability) &&
    (!Number.isInteger(request.initialB2B) || request.initialB2B < 0 || request.initialB2B > 65535)
  ) {
    errors.push('initial_b2b_invalid');
  }
  if (!Number.isInteger(request.workers) || request.workers < 1) {
    errors.push('worker_count_invalid');
  }
  return [...new Set(errors)];
}

export function buildV2Command(request: BuildV2Request): string {
  request = normalizeBuildV2Request(request);
  const tokens = ['clearra', 'build', ...buildV2CommandPath(request.capability)];
  const source = buildV2SourceKind(request.capability);
  if (source === 'mask') {
    tokens.push(
      '--base-mask',
      boardMaskHex(trimBuildV2Mask(request.baseMask, request.height)),
      '--target-mask',
      boardMaskHex(trimBuildV2Mask(request.targetMask, request.height)),
      '--height',
      String(request.height)
    );
    if (request.sourcePieceCount !== null) {
      tokens.push('--source-pieces', String(request.sourcePieceCount));
    }
  } else if (source === 'target-document') {
    tokens.push(
      '--target-format',
      request.targetFormat,
      '--target-document',
      quoteBuildV2Token(request.targetDocument.trim())
    );
  } else {
    tokens.push(
      '--solution-format',
      request.solutionFormat,
      '--solution-document',
      quoteBuildV2Token(request.solutionDocument.trim())
    );
  }
  const parsedQueue = parseBrowserQueueInput(request.queue);
  tokens.push(
    parsedQueue?.kind === 'pattern' ? '--patterns' : '--queue',
    quoteBuildV2Token(parsedQueue?.source ?? request.queue.trim())
  );
  if (request.holdEnabled) tokens.push('--hold', request.holdPiece);
  else tokens.push('--no-hold');
  tokens.push(
    '--queue-knowledge',
    request.queueKnowledge,
    '--objective',
    request.objective,
    '--rule',
    request.rule,
    '--backend',
    'cpu',
    '--no-backend-fallback',
    '--workers',
    String(request.workers),
    '--cpu-warmup'
  );
  if (request.useAllLogicalProcessors) tokens.push('--use-all-logical-processors');
  if (buildV2ScoreCapable(request.capability)) {
    tokens.push(
      '--score-profile',
      request.scoreProfile,
      '--initial-b2b',
      String(request.initialB2B)
    );
  }
  return tokens.join(' ');
}

export function buildV2RequestForDesktop(
  request: BuildV2Request,
  language: 'en' | 'ko'
): ClearraDesktopBuildV2Request {
  request = normalizeBuildV2Request(request);
  const parsedQueue = parseBrowserQueueInput(request.queue);
  const source = buildV2SourceKind(request.capability);
  return buildDesktopBuildV2Request({
    language,
    capability_id: request.capability,
    ...(source === 'mask'
      ? {
          base_mask: boardMaskHex(trimBuildV2Mask(request.baseMask, request.height)),
          target_mask: boardMaskHex(trimBuildV2Mask(request.targetMask, request.height)),
          visible_height: request.height,
          ...(request.sourcePieceCount === null
            ? {}
            : { source_piece_count: request.sourcePieceCount })
        }
      : source === 'target-document'
        ? {
            target_format: request.targetFormat,
            target_document: request.targetDocument.trim()
          }
        : {
            solution_format: request.solutionFormat,
            solution_document: request.solutionDocument.trim()
          }),
    queue: parsedQueue?.kind === 'fixed' ? parsedQueue.source : '',
    patterns: parsedQueue?.kind === 'pattern' ? parsedQueue.source : '',
    queue_knowledge: request.queueKnowledge,
    hold_enabled: request.holdEnabled,
    hold_piece: request.holdPiece,
    objective: request.objective,
    ...(buildV2ScoreCapable(request.capability)
      ? { score_profile: request.scoreProfile, initial_b2b: request.initialB2B }
      : {}),
    rule: request.rule,
    workers: request.workers,
    use_all_logical_processors: request.useAllLogicalProcessors
  });
}

export function trimBuildV2Mask(mask: bigint, height: number): bigint {
  const cells = Math.max(0, Math.min(240, Math.trunc(height) * 10));
  return cells === 0 ? 0n : mask & ((1n << BigInt(cells)) - 1n);
}

function buildV2CommandPath(capability: BuildV2Capability): string[] {
  return capability.startsWith('build.evaluate.')
    ? ['evaluate', capability.slice('build.evaluate.'.length)]
    : [capability.slice('build.'.length)];
}

function validBuildV2Document(format: BuildV2DocumentFormat, document: string): boolean {
  const value = document.trim();
  if (!value || new TextEncoder().encode(value).byteLength > 16 * 1024 * 1024) return false;
  return format === 'ctk3'
    ? /^ctk3(?:b_|_|@)/u.test(value)
    : /^(?:v115|[Ddm]115)@/u.test(value);
}

function quoteBuildV2Token(value: string): string {
  return /^[^\s"'\\]+$/u.test(value)
    ? value
    : `"${value.replaceAll('\\', '\\\\').replaceAll('"', '\\"')}"`;
}
