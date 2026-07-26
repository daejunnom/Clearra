import type { ClearraWasmSearchPathStep } from '../wasm/wasmCommandClient';

export type SetupCandidatePriority = 'all' | 'build' | 'pc';
export type SetupLengthPreference = 'auto' | 'longer' | 'shorter';
export type SetupSearchMode = 'oracle' | 'qb';

export type SetupFinderRequest = {
  searchMode: SetupSearchMode;
  remaining: string;
  qbQueue: string;
  allowPostCycleBorrow: boolean;
  candidatePriority: SetupCandidatePriority;
  lengthPreference: SetupLengthPreference;
};

export type SetupFinderValidationCode =
  | 'setup_residue_count_invalid'
  | 'setup_residue_piece_invalid'
  | 'setup_residue_duplicate_invalid'
  | 'setup_cycle_borrow_invalid'
  | 'setup_qb_count_invalid'
  | 'setup_qb_piece_invalid'
  | 'setup_qb_duplicate_invalid'
  | 'setup_qb_combined_count_invalid';

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
    remaining: PIECES,
    qbQueue: '',
    allowPostCycleBorrow: false,
    candidatePriority: 'all',
    lengthPreference: 'auto'
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

export function explicitSetupHold(remaining: string): string | null {
  const normalized = normalizedSetupResidue(remaining);
  return [...PIECES].find((piece) => normalized.split(piece).length - 1 === 2) ?? null;
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
  if (repeated.length > 1 || repeated.some((count) => count > 2)) {
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
    request.candidatePriority === 'all' ? '' : `--priority ${request.candidatePriority}`,
    request.lengthPreference === 'auto' ? '' : `--setup-length ${request.lengthPreference}`,
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
