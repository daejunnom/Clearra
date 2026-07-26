import type { ClearraWasmSearchPathStep } from '../wasm/wasmCommandClient';
import type { RuleProfile } from './solverWorkspaceModel';

export type SetupCandidatePriority = 'all' | 'build' | 'pc';
export type SetupLengthPreference = 'auto' | 'longer' | 'shorter';
export type SetupSearchMode = 'oracle' | 'qb';

export type SetupFinderRequest = {
  searchMode: SetupSearchMode;
  rule: RuleProfile;
  remaining: string;
  qbQueue: string;
  allowPostCycleBorrow: boolean;
  candidatePriority: SetupCandidatePriority;
  lengthPreference: SetupLengthPreference;
  maxSetupPieces: number;
};

export type SetupFinderValidationCode =
  | 'setup_residue_count_invalid'
  | 'setup_residue_piece_invalid'
  | 'setup_residue_duplicate_invalid'
  | 'setup_cycle_borrow_invalid'
  | 'setup_qb_count_invalid'
  | 'setup_qb_piece_invalid'
  | 'setup_qb_duplicate_invalid'
  | 'setup_max_pieces_invalid';

export type SetupPathDetailRequest = {
  conditionId: string;
  setupId: string;
};

export type SetupPathDetailState = {
  status: 'loading' | 'complete' | 'failed';
  paths: ClearraWasmSearchPathStep[][];
  complete: boolean;
  error: string | null;
};

const PIECES = 'IOTSZJL';

export function createDefaultSetupFinderRequest(): SetupFinderRequest {
  return {
    searchMode: 'oracle',
    rule: 'srs-plus',
    remaining: PIECES,
    qbQueue: '',
    allowPostCycleBorrow: false,
    candidatePriority: 'all',
    lengthPreference: 'auto',
    maxSetupPieces: 9
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
  const codes: SetupFinderValidationCode[] = [];
  if ([...normalized].some((piece) => !PIECES.includes(piece))) {
    codes.push('setup_residue_piece_invalid');
  }
  if (!setupCycle(normalized)) codes.push('setup_residue_count_invalid');
  const repeated = [...PIECES]
    .map((piece) => normalized.split(piece).length - 1)
    .filter((count) => count > 1);
  if (repeated.length > 0) {
    codes.push('setup_residue_duplicate_invalid');
  }
  if (request.allowPostCycleBorrow && setupCycle(normalized) !== 7) {
    codes.push('setup_cycle_borrow_invalid');
  }
  if (request.searchMode === 'qb') {
    if (normalizedQb.length !== nextSetupCycleRemainingCount(normalized)) {
      codes.push('setup_qb_count_invalid');
    }
    if ([...normalizedQb].some((piece) => !PIECES.includes(piece))) {
      codes.push('setup_qb_piece_invalid');
    }
    const qbCounts = [...PIECES].map(
      (piece) => normalizedQb.split(piece).length - 1
    );
    if (qbCounts.some((count) => count > 2)
      || qbCounts.filter((count) => count === 2).length > 1) {
      codes.push('setup_qb_duplicate_invalid');
    }
  }
  if (!Number.isInteger(request.maxSetupPieces)
    || request.maxSetupPieces < 1
    || request.maxSetupPieces > 10) {
    codes.push('setup_max_pieces_invalid');
  }
  return [...new Set(codes)];
}

export function buildSetupFinderCommand(request: SetupFinderRequest): string {
  const remaining = normalizedSetupResidue(request.remaining);
  return [
    'clearra setup',
    `--remaining ${remaining}`,
    request.searchMode === 'qb'
      ? `--mode qb --qb ${normalizedSetupResidue(request.qbQueue)}`
      : '',
    `--rule ${request.rule}`,
    request.candidatePriority === 'all' ? '' : `--priority ${request.candidatePriority}`,
    request.lengthPreference === 'auto' ? '' : `--setup-length ${request.lengthPreference}`,
    `--max-setup-pieces ${request.maxSetupPieces}`,
    request.allowPostCycleBorrow ? '--allow-post-cycle-borrow' : ''
  ].filter(Boolean).join(' ');
}

export function buildSetupPathDetailCommand(
  request: SetupFinderRequest,
  detail: SetupPathDetailRequest
): string {
  return [
    buildSetupFinderCommand(request),
    `--paths-for ${detail.setupId}`,
    `--condition ${detail.conditionId}`
  ].join(' ');
}

export function setupPathDetailKey(detail: SetupPathDetailRequest): string {
  return `${detail.conditionId}:${detail.setupId}`;
}
