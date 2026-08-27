import {
  buildDesktopSetupScoreRequest,
  type ClearraDesktopSetupScoreRequest
} from '../host/clearraDesktopHost.ts';
import {
  defaultWorkerCount,
  parseBrowserQueueInput
} from './solverWorkspaceModel.ts';

export type SetupScoreDocumentFormat = 'ctk3' | 'fumen';
export type SetupScoreSourceKind = 'queue' | 'patterns';
export type SetupScoreProfile = 'tetrio' | 'guideline' | 'jstris-ultra';
export type SetupScoreRule = 'srs-plus' | 'srs' | 'srs-x' | 'jstris-180' | 'no-kick';

export type SetupScoreRequest = {
  documentFormat: SetupScoreDocumentFormat;
  document: string;
  setupSourceKind: SetupScoreSourceKind;
  setupSource: string;
  solutionSourceKind: SetupScoreSourceKind;
  solutionSource: string;
  clearHeight: number;
  holdEnabled: boolean;
  scoreProfile: SetupScoreProfile;
  initialB2B: number;
  rule: SetupScoreRule;
  maxPatterns: number;
  workers: number;
  useAllLogicalProcessors: boolean;
};

export type SetupScoreValidationCode =
  | 'document_invalid'
  | 'setup_source_invalid'
  | 'solution_source_invalid'
  | 'clear_height_invalid'
  | 'initial_b2b_invalid'
  | 'max_patterns_invalid'
  | 'worker_count_invalid';

export function createDefaultSetupScoreRequest(): SetupScoreRequest {
  return {
    documentFormat: 'ctk3',
    document: '',
    setupSourceKind: 'queue',
    setupSource: 'I',
    solutionSourceKind: 'queue',
    solutionSource: 'OTSJ',
    clearHeight: 4,
    holdEnabled: true,
    scoreProfile: 'tetrio',
    initialB2B: 0,
    rule: 'srs-plus',
    maxPatterns: 100_000,
    workers: defaultWorkerCount(),
    useAllLogicalProcessors: false
  };
}

export function setupScoreValidationCodes(
  request: SetupScoreRequest
): SetupScoreValidationCode[] {
  const errors: SetupScoreValidationCode[] = [];
  if (!validSetupScoreDocument(request.documentFormat, request.document)) {
    errors.push('document_invalid');
  }
  if (!validSetupScoreSource(request.setupSourceKind, request.setupSource, false)) {
    errors.push('setup_source_invalid');
  }
  if (!validSetupScoreSource(request.solutionSourceKind, request.solutionSource, true)) {
    errors.push('solution_source_invalid');
  }
  if (
    !Number.isInteger(request.clearHeight) ||
    request.clearHeight < 1 ||
    request.clearHeight > 6
  ) {
    errors.push('clear_height_invalid');
  }
  if (
    !Number.isSafeInteger(request.initialB2B) ||
    request.initialB2B < 0 ||
    request.initialB2B > 0xffff_ffff
  ) {
    errors.push('initial_b2b_invalid');
  }
  if (
    !Number.isInteger(request.maxPatterns) ||
    request.maxPatterns < 1 ||
    request.maxPatterns > 100_000
  ) {
    errors.push('max_patterns_invalid');
  }
  if (
    !Number.isInteger(request.workers) ||
    request.workers < 1
  ) {
    errors.push('worker_count_invalid');
  }
  return [...new Set(errors)];
}

export function buildSetupScoreCommand(request: SetupScoreRequest): string {
  const tokens = [
    'clearra',
    'setup',
    'score',
    '--document-format',
    request.documentFormat,
    '--document',
    quoteCommandToken(request.document.trim()),
    request.setupSourceKind === 'queue' ? '--setup-queue' : '--setup-patterns',
    quoteCommandToken(normalizedSetupScoreSource(request.setupSource)),
    request.solutionSourceKind === 'queue' ? '--solution-queue' : '--solution-patterns',
    quoteCommandToken(normalizedSetupScoreSource(request.solutionSource)),
    '--clear',
    String(request.clearHeight),
    request.holdEnabled ? '--hold' : '--no-hold',
    '--score-profile',
    request.scoreProfile,
    '--initial-b2b',
    String(request.initialB2B),
    '--rule',
    request.rule,
    '--max-patterns',
    String(request.maxPatterns),
    '--backend',
    'cpu',
    '--no-backend-fallback'
  ];
  if (request.useAllLogicalProcessors) {
    tokens.push('--use-all-logical-processors');
  } else {
    tokens.push('--workers', String(request.workers));
  }
  return tokens.join(' ');
}

export function setupScoreRequestForDesktop(
  request: SetupScoreRequest,
  language: 'en' | 'ko'
): ClearraDesktopSetupScoreRequest {
  const setupSource = normalizedSetupScoreSource(request.setupSource);
  const solutionSource = normalizedSetupScoreSource(request.solutionSource);
  return buildDesktopSetupScoreRequest({
    language,
    document_format: request.documentFormat,
    document: request.document.trim(),
    setup_queue: request.setupSourceKind === 'queue' ? setupSource : '',
    setup_patterns: request.setupSourceKind === 'patterns' ? setupSource : '',
    solution_queue: request.solutionSourceKind === 'queue' ? solutionSource : '',
    solution_patterns: request.solutionSourceKind === 'patterns' ? solutionSource : '',
    clear_height: request.clearHeight,
    hold_enabled: request.holdEnabled,
    score_profile: request.scoreProfile,
    initial_b2b: request.initialB2B,
    rule: request.rule,
    max_patterns: request.maxPatterns,
    workers: request.useAllLogicalProcessors ? 0 : request.workers,
    use_all_logical_processors: request.useAllLogicalProcessors
  });
}

function validSetupScoreDocument(
  format: SetupScoreDocumentFormat,
  document: string
): boolean {
  const value = document.trim();
  if (!value || new TextEncoder().encode(value).byteLength > 16 * 1024 * 1024) {
    return false;
  }
  return format === 'ctk3'
    ? /^ctk3(?:b_|_|@)/u.test(value)
    : /^(?:v115|[Ddm]115)@/u.test(value);
}

function validSetupScoreSource(
  kind: SetupScoreSourceKind,
  source: string,
  continuation: boolean
): boolean {
  const parsed = parseBrowserQueueInput(source);
  if (!parsed || (kind === 'queue' && parsed.kind !== 'fixed')) return false;
  return !continuation || parsed.sequenceLength <= 16;
}

function normalizedSetupScoreSource(source: string): string {
  return parseBrowserQueueInput(source)?.source ?? source.trim();
}

function quoteCommandToken(value: string): string {
  return /^[^\s"'\\]+$/u.test(value)
    ? value
    : `"${value.replaceAll('\\', '\\\\').replaceAll('"', '\\"')}"`;
}
