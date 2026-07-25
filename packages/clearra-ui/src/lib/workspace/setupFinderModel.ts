export type SetupFinderRequest = {
  remaining: string;
  allowPostCycleBorrow: boolean;
};

export type SetupFinderValidationCode =
  | 'setup_residue_count_invalid'
  | 'setup_residue_piece_invalid'
  | 'setup_residue_duplicate_invalid'
  | 'setup_cycle_borrow_invalid';

const PIECES = 'IOTSZJL';

export function createDefaultSetupFinderRequest(): SetupFinderRequest {
  return {
    remaining: PIECES,
    allowPostCycleBorrow: false
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
  const codes: SetupFinderValidationCode[] = [];
  if (!setupCycle(normalized)) codes.push('setup_residue_count_invalid');
  if ([...normalized].some((piece) => !PIECES.includes(piece))) {
    codes.push('setup_residue_piece_invalid');
  }
  const repeated = [...PIECES]
    .map((piece) => normalized.split(piece).length - 1)
    .filter((count) => count > 1);
  if (repeated.length > 1 || repeated.some((count) => count > 2)) {
    codes.push('setup_residue_duplicate_invalid');
  }
  if (request.allowPostCycleBorrow && setupCycle(normalized) !== 7) {
    codes.push('setup_cycle_borrow_invalid');
  }
  return [...new Set(codes)];
}

export function buildSetupFinderCommand(request: SetupFinderRequest): string {
  const remaining = normalizedSetupResidue(request.remaining);
  return [
    'clearra setup',
    `--remaining ${remaining}`,
    request.allowPostCycleBorrow ? '--allow-post-cycle-borrow' : ''
  ].filter(Boolean).join(' ');
}
