import type { ClearraDesktopCliCommandRequest } from '../host/clearraDesktopHost.ts';
import { defaultWorkerCount } from './solverWorkspaceModel.ts';
import {
  cliCommandRequestForDesktop,
  serializeCliCommandArguments
} from './cliCommandModel.ts';

export type SpinStructureMode = 'search' | 'cover' | 'guaranteed';
export type SpinStructureProfile =
  | 't-spins'
  | 't-spins-plus'
  | 'all-mini'
  | 'all-mini-plus'
  | 'all-spin'
  | 'all-spin-plus';
export type SpinStructureLines =
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
export type SpinStructureRule =
  | 'srs-plus'
  | 'srs'
  | 'srs-x'
  | 'jstris-180'
  | 'no-kick';
export type SpinStructureMinimality = 'subset-minimal' | 'minimum-piece-count';
export type SpinStructurePiece = 'I' | 'O' | 'T' | 'S' | 'Z' | 'J' | 'L';

export type SpinStructureRequest = {
  mode: SpinStructureMode;
  boardMaskV1: string;
  visibleHeight: number;
  inventory: string;
  spinProfile: SpinStructureProfile;
  lines: SpinStructureLines;
  fillBottom: number;
  fillTop: number;
  rule: SpinStructureRule;
  maxPlacements: number;
  minimality: SpinStructureMinimality;
  maxPatterns: number;
  finalPiece: SpinStructurePiece;
  dependencyReport: boolean;
  workers: number;
  useAllLogicalProcessors: boolean;
};

export type SpinStructureValidationCode =
  | 'board_mask_invalid'
  | 'height_invalid'
  | 'board_outside_height'
  | 'inventory_invalid'
  | 'fill_window_invalid'
  | 'max_placements_invalid'
  | 'max_patterns_invalid'
  | 'final_piece_invalid'
  | 'worker_count_invalid';

export const EMPTY_SPIN_BOARD_MASK_V1 = '0'.repeat(60);

export function createDefaultSpinStructureRequest(): SpinStructureRequest {
  return {
    mode: 'search',
    boardMaskV1: EMPTY_SPIN_BOARD_MASK_V1,
    visibleHeight: 8,
    inventory: 'T',
    spinProfile: 't-spins',
    lines: '1+',
    fillBottom: 0,
    fillTop: 5,
    rule: 'srs-plus',
    maxPlacements: 1,
    minimality: 'subset-minimal',
    maxPatterns: 100_000,
    finalPiece: 'T',
    dependencyReport: false,
    workers: defaultWorkerCount(),
    useAllLogicalProcessors: false
  };
}

export function spinStructureValidationCodes(
  request: SpinStructureRequest
): SpinStructureValidationCode[] {
  const errors: SpinStructureValidationCode[] = [];
  const mask = request.boardMaskV1.trim();
  const inventory = normalizedSpinInventory(request.inventory);
  if (!/^[0-9a-f]{60}$/u.test(mask)) errors.push('board_mask_invalid');
  if (
    !Number.isInteger(request.visibleHeight) ||
    request.visibleHeight < 4 ||
    request.visibleHeight > 24
  ) {
    errors.push('height_invalid');
  } else if (/^[0-9a-f]{60}$/u.test(mask) && spinBoardMinimumHeight(mask) > request.visibleHeight) {
    errors.push('board_outside_height');
  }
  if (!inventory || !/^[IOTSZJL]+$/u.test(inventory) || inventory.length > 255) {
    errors.push('inventory_invalid');
  }
  if (
    !Number.isInteger(request.fillBottom) ||
    !Number.isInteger(request.fillTop) ||
    request.fillBottom < 0 ||
    request.fillBottom >= request.fillTop ||
    request.fillTop > request.visibleHeight
  ) {
    errors.push('fill_window_invalid');
  }
  if (
    !Number.isInteger(request.maxPlacements) ||
    request.maxPlacements < 1 ||
    request.maxPlacements > Math.min(255, inventory.length)
  ) {
    errors.push('max_placements_invalid');
  }
  if (
    request.mode !== 'search' &&
    (!Number.isInteger(request.maxPatterns) ||
      request.maxPatterns < 1 ||
      request.maxPatterns > 100_000)
  ) {
    errors.push('max_patterns_invalid');
  }
  if (
    request.mode === 'guaranteed' &&
    (!inventory.includes(request.finalPiece) ||
      (request.spinProfile.startsWith('t-spins') && request.finalPiece !== 'T'))
  ) {
    errors.push('final_piece_invalid');
  }
  if (!Number.isInteger(request.workers) || request.workers < 1) {
    errors.push('worker_count_invalid');
  }
  return [...new Set(errors)];
}

export function buildSpinStructureCommandArguments(request: SpinStructureRequest): string[] {
  const tokens = [
    'clearra',
    'spin-structure',
    request.mode,
    '--board-mask-v1',
    request.boardMaskV1.trim(),
    '--height',
    String(request.visibleHeight),
    '--pieces',
    normalizedSpinInventory(request.inventory),
    '--spin-profile',
    request.spinProfile,
    '--lines',
    request.lines,
    '--fill-bottom',
    String(request.fillBottom),
    '--fill-top',
    String(request.fillTop),
    '--rule',
    request.rule,
    '--max-placements',
    String(request.maxPlacements),
    '--minimality',
    request.minimality
  ];
  if (request.mode === 'cover') {
    tokens.push('--objective', 'min-cover', '--max-patterns', String(request.maxPatterns));
  } else if (request.mode === 'guaranteed') {
    tokens.push(
      '--final-piece',
      request.finalPiece,
      '--max-patterns',
      String(request.maxPatterns),
      request.dependencyReport ? '--dependency-report' : '--no-dependency-report'
    );
  }
  if (request.useAllLogicalProcessors) {
    tokens.push('--use-all-logical-processors');
  } else {
    tokens.push('--workers', String(request.workers));
  }
  return tokens;
}

export function buildSpinStructureCommand(request: SpinStructureRequest): string {
  return serializeCliCommandArguments(buildSpinStructureCommandArguments(request));
}

export function spinStructureRequestForDesktop(
  request: SpinStructureRequest,
  language: 'en' | 'ko'
): ClearraDesktopCliCommandRequest {
  return cliCommandRequestForDesktop(buildSpinStructureCommandArguments(request), language);
}

export function normalizedSpinInventory(value: string): string {
  return value.toUpperCase().replace(/[\s,]+/gu, '');
}

export function spinBoardMinimumHeight(mask: string): number {
  const value = BigInt(`0x${mask}`);
  return value === 0n ? 4 : Math.max(4, Math.ceil(value.toString(2).length / 10));
}
