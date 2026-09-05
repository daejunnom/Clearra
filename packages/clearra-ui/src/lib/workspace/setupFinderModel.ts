import type { ClearraWasmSearchPathStep } from '../wasm/wasmCommandClient.ts';
import type { ClearraDesktopCliCommandRequest } from '../host/clearraDesktopHost.ts';
import type {
  WorkspaceDeveloperFailureEvidence,
  WorkspacePublicFailure
} from './workspacePublicFailure';
import type { QueueKnowledge, RuleProfile } from './solverWorkspaceModel.ts';
import {
  cliCommandRequestForDesktop,
  serializeCliCommandArguments
} from './cliCommandModel.ts';

export type SetupCandidatePriority = 'all' | 'build' | 'pc';
export type SetupLengthPreference = 'auto' | 'longer' | 'shorter';
export type SetupSearchMode = 'oracle' | 'qb';

export type SetupFinderRequest = {
  searchMode: SetupSearchMode;
  queueKnowledge: QueueKnowledge;
  rule: RuleProfile;
  remaining: string;
  qbQueue: string;
  nextCycleRemaining: string;
  allowPostCycleBorrow: boolean;
  candidatePriority: SetupCandidatePriority;
  lengthPreference: SetupLengthPreference;
  maxSetupPieces: number;
  tablebaseEnabled: boolean;
  useAllLogicalProcessors: boolean;
};

export type SetupFinderValidationCode =
  | 'setup_residue_count_invalid'
  | 'setup_residue_piece_invalid'
  | 'setup_residue_duplicate_invalid'
  | 'setup_cycle_borrow_invalid'
  | 'setup_qb_count_invalid'
  | 'setup_qb_piece_invalid'
  | 'setup_qb_duplicate_invalid'
  | 'setup_qb_combined_count_invalid'
  | 'setup_next_cycle_count_invalid'
  | 'setup_next_cycle_piece_invalid'
  | 'setup_next_cycle_duplicate_invalid'
  | 'setup_max_pieces_invalid';

export type SetupPathDetailRequest = {
  conditionId: string;
  setupId: string;
};

export type SetupPathDetailState = {
  status: 'loading' | 'complete' | 'failed';
  paths: ClearraWasmSearchPathStep[][];
  complete: boolean;
  publicFailures: WorkspacePublicFailure[];
  developerFailure: WorkspaceDeveloperFailureEvidence | null;
};

const PIECES = 'IOTSZJL';

export function createDefaultSetupFinderRequest(): SetupFinderRequest {
  return {
    searchMode: 'oracle',
    queueKnowledge: 'oracle',
    rule: 'srs-plus',
    remaining: PIECES,
    qbQueue: '',
    nextCycleRemaining: '',
    allowPostCycleBorrow: false,
    candidatePriority: 'all',
    lengthPreference: 'auto',
    maxSetupPieces: 9,
    tablebaseEnabled: false,
    useAllLogicalProcessors: false
  };
}

export function normalizedSetupResidue(value: string): string {
  return value
    .toUpperCase()
    .split('')
    .filter((value) => !/\s|,/.test(value))
    .join('');
}

export function setupCycle(remaining: string): number | null {
  switch (normalizedSetupResidue(remaining).length) {
    case 7: return 1;
    case 4: return 2;
    case 1: return 3;
    case 5: return 4;
    case 2: return 5;
    case 6: return 6;
    case 3: return 7;
    default: return null;
  }
}

export function nextSetupCycleRemainingCount(remaining: string): number | null {
  switch (setupCycle(remaining)) {
    case 1: return 4;
    case 2: return 1;
    case 3: return 5;
    case 4: return 2;
    case 5: return 6;
    case 6: return 3;
    case 7: return 7;
    default: return null;
  }
}

export function setupFinderValidationCodes(
  request: SetupFinderRequest
): SetupFinderValidationCode[] {
  const normalized = normalizedSetupResidue(request.remaining);
  const normalizedQb = normalizedSetupResidue(request.qbQueue);
  const normalizedNextCycle = normalizedSetupResidue(request.nextCycleRemaining);
  const codes: SetupFinderValidationCode[] = [];
  if ([...normalized].some((piece) => !PIECES.includes(piece))) {
    codes.push('setup_residue_piece_invalid');
  }
  if (!setupCycle(normalized)) codes.push('setup_residue_count_invalid');
  const repeated = [...PIECES].map(
    (piece) => normalized.split(piece).length - 1
  );
  if (repeated.some((count) => count > 2)
    || repeated.filter((count) => count === 2).length > 1) {
    codes.push('setup_residue_duplicate_invalid');
  }
  if (request.allowPostCycleBorrow && setupCycle(normalized) !== 7) {
    codes.push('setup_cycle_borrow_invalid');
  }
  if (request.searchMode === 'qb') {
    if (normalizedQb.length < 1) {
      codes.push('setup_qb_count_invalid');
    }
    if ([...normalizedQb].some((piece) => !PIECES.includes(piece))) {
      codes.push('setup_qb_piece_invalid');
    }
    if ([...new Set(normalizedQb)].length !== normalizedQb.length) {
      codes.push('setup_qb_duplicate_invalid');
    }
    if (normalized.length + normalizedQb.length > 7) {
      codes.push('setup_qb_combined_count_invalid');
    }
  }
  if (normalizedNextCycle.length > 0) {
    if (normalizedNextCycle.length !== nextSetupCycleRemainingCount(normalized)) {
      codes.push('setup_next_cycle_count_invalid');
    }
    if ([...normalizedNextCycle].some((piece) => !PIECES.includes(piece))) {
      codes.push('setup_next_cycle_piece_invalid');
    }
    const nextCycleCounts = [...PIECES].map(
      (piece) => normalizedNextCycle.split(piece).length - 1
    );
    if (nextCycleCounts.some((count) => count > 2)
      || nextCycleCounts.filter((count) => count === 2).length > 1) {
      codes.push('setup_next_cycle_duplicate_invalid');
    }
  }
  if (!Number.isInteger(request.maxSetupPieces)
    || request.maxSetupPieces < 1
    || request.maxSetupPieces > 10) {
    codes.push('setup_max_pieces_invalid');
  }
  return [...new Set(codes)];
}

export function buildSetupFinderCommand(
  request: SetupFinderRequest,
  automaticWorkerLimit?: number
): string {
  return serializeCliCommandArguments(
    buildSetupFinderCommandArguments(request, automaticWorkerLimit)
  );
}

export function buildSetupFinderCommandArguments(
  request: SetupFinderRequest,
  automaticWorkerLimit?: number
): string[] {
  return buildSetupFinderCommandArgumentsWithRoute(request, automaticWorkerLimit, true);
}

function buildSetupFinderCommandArgumentsWithRoute(
  request: SetupFinderRequest,
  automaticWorkerLimit: number | undefined,
  canonicalRankedRoute: boolean
): string[] {
  const remaining = normalizedSetupResidue(request.remaining);
  const tokens = [
    'clearra',
    ...(canonicalRankedRoute
      ? ['setup', request.candidatePriority === 'all' ? 'joint' : request.candidatePriority]
      : ['setup-finder']),
    '--remaining',
    remaining
  ];
  if (request.searchMode === 'qb') {
    tokens.push('--mode', 'qb', '--qb', normalizedSetupResidue(request.qbQueue));
  }
  tokens.push('--queue-knowledge', request.queueKnowledge);
  const nextCycleRemaining = normalizedSetupResidue(request.nextCycleRemaining);
  if (nextCycleRemaining) {
    tokens.push('--next-cycle-remaining', nextCycleRemaining);
  }
  tokens.push('--rule', request.rule);
  tokens.push(request.tablebaseEnabled ? '--tablebase' : '--no-tablebase');
  if (!canonicalRankedRoute && request.candidatePriority !== 'all') {
    tokens.push('--priority', request.candidatePriority);
  }
  if (request.lengthPreference !== 'auto') {
    tokens.push('--setup-length', request.lengthPreference);
  }
  tokens.push('--max-setup-pieces', String(request.maxSetupPieces));
  if (automaticWorkerLimit !== undefined) {
    tokens.push('--auto-workers', String(Math.max(1, Math.trunc(automaticWorkerLimit))));
  }
  if (request.allowPostCycleBorrow) tokens.push('--allow-post-cycle-borrow');
  if (request.useAllLogicalProcessors && automaticWorkerLimit !== undefined) {
    tokens.push('--use-all-cpu-threads');
  }
  return tokens;
}

export function buildSetupPathDetailCommand(
  request: SetupFinderRequest,
  detail: SetupPathDetailRequest,
  automaticWorkerLimit?: number
): string {
  return serializeCliCommandArguments(
    buildSetupPathDetailCommandArguments(request, detail, automaticWorkerLimit)
  );
}

export function buildSetupPathDetailCommandArguments(
  request: SetupFinderRequest,
  detail: SetupPathDetailRequest,
  automaticWorkerLimit?: number
): string[] {
  return [
    ...buildSetupFinderCommandArgumentsWithRoute(
      { ...request, useAllLogicalProcessors: false },
      automaticWorkerLimit,
      false
    ),
    '--paths-for',
    detail.setupId,
    '--condition',
    detail.conditionId
  ];
}

export function setupFinderRequestForDesktop(
  request: SetupFinderRequest,
  language: 'en' | 'ko',
  workers: number,
  detail?: SetupPathDetailRequest
): ClearraDesktopCliCommandRequest {
  const arguments_ = detail === undefined
    ? buildSetupFinderCommandArguments(request, workers)
    : buildSetupPathDetailCommandArguments(request, detail, workers);
  return cliCommandRequestForDesktop(arguments_, language);
}

export function setupPathDetailKey(detail: SetupPathDetailRequest): string {
  return `${detail.conditionId}:${detail.setupId}`;
}
