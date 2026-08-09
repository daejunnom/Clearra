import type { ClearraDesktopRequest } from '../host';

export type ScoreMode = 'tiling' | 'off' | 'minimum-cover' | 'summary' | 'failed-queue';
export type ScoreProfile = 'guideline' | 'jstris-ultra' | 'tetrio';
export type RuleProfile = 'srs-plus' | 'srs' | 'srs-x' | 'jstris-180';
export type SpinProfile =
  | 't-spins'
  | 't-spins-plus'
  | 'all-spin'
  | 'all-spin-plus'
  | 'all-mini'
  | 'all-mini-plus';
export type SearchBackend = 'auto' | 'cpu' | 'gpu' | 'hybrid';
export type QueueKnowledge = 'oracle' | 'visible-7';
export type SolverHoldPiece = 'empty' | 'I' | 'O' | 'T' | 'S' | 'Z' | 'J' | 'L';

export type SolverWorkspaceRequest = {
  lines: number;
  boardMask: bigint;
  queue: string;
  holdEnabled: boolean;
  /** Omitted callers keep the historical empty-hold scenario. */
  holdPiece?: SolverHoldPiece;
  queueKnowledge: QueueKnowledge;
  scoreMode: ScoreMode;
  scoreProfile: ScoreProfile;
  rule: RuleProfile;
  spinProfile: SpinProfile;
  preserveB2B: boolean;
  initialB2B: number;
  solutionProbabilities: boolean;
  backend: SearchBackend;
  gpuDevice: string;
  workers: number;
  useAllLogicalProcessors: boolean;
  tablebaseEnabled: boolean;
  precomputeBuildDependencies: boolean;
  /** Exact pattern-universe ceiling for generated factorized inputs. */
  maxPatterns?: number;
};

export type WorkspaceValidationCode =
  | 'queue_invalid'
  | 'target_lines_invalid'
  | 'scenario_not_tileable'
  | 'scenario_supply_mismatch'
  | 'scenario_full'
  | 'worker_count_invalid'
  | 'initial_b2b_invalid'
  | 'gpu_device_invalid';

export function createDefaultWorkspaceRequest(): SolverWorkspaceRequest {
  return {
    lines: 4,
    boardMask: 0n,
    queue: '',
    holdEnabled: true,
    queueKnowledge: 'oracle',
    scoreMode: 'off',
    scoreProfile: 'tetrio',
    rule: 'srs-plus',
    spinProfile: 't-spins',
    preserveB2B: false,
    initialB2B: 0,
    solutionProbabilities: false,
    backend: 'auto',
    gpuDevice: 'auto',
    workers: defaultWorkerCount(),
    useAllLogicalProcessors: false,
    tablebaseEnabled: false,
    precomputeBuildDependencies: false
  };
}

export function normalizeWorkspaceRequest(
  request: SolverWorkspaceRequest
): SolverWorkspaceRequest {
  if (request.scoreMode === 'failed-queue') {
    return {
      ...request,
      initialB2B: 0,
      solutionProbabilities: false
    };
  }
  if (request.scoreMode !== 'tiling') return request;
  return {
    ...request,
    queueKnowledge: 'oracle',
    rule: 'srs-plus',
    preserveB2B: false,
    initialB2B: 0,
    solutionProbabilities: false,
    tablebaseEnabled: false,
    precomputeBuildDependencies: false
  };
}

export function defaultWorkerCount(
  hardwareConcurrency?: number,
  useAllLogicalProcessors = false
): number {
  const logicalProcessors = logicalProcessorCount(hardwareConcurrency);
  return useAllLogicalProcessors ? logicalProcessors : Math.max(1, logicalProcessors - 1);
}

export function defaultBrowserWorkerCount(
  hardwareConcurrency?: number,
  useAllLogicalProcessors = false
): number {
  return defaultWorkerCount(hardwareConcurrency, useAllLogicalProcessors);
}

export function logicalProcessorCount(hardwareConcurrency?: number): number {
  const reported = hardwareConcurrency ?? globalThis.navigator?.hardwareConcurrency ?? 1;
  return Number.isFinite(reported) ? Math.max(1, Math.floor(reported)) : 1;
}

export function normalizeQueueInput(value: string): string {
  return value.toUpperCase().replace(/[\s,]+/g, '');
}

export type BrowserQueueInput = {
  source: string;
  kind: 'fixed' | 'pattern';
  sequenceLength: number;
};

const STANDARD_PIECE_LETTERS = 'IOTSZLJ';

export function parseBrowserQueueInput(value: string): BrowserQueueInput | null {
  const input = normalizeQueueInput(value);
  if (!input) return null;

  let cursor = 0;
  let canonical = '';
  let pattern = false;
  let alternativeHasAtom = false;
  let alternativeLength = 0;
  let sequenceLength: number | null = null;

  while (cursor < input.length) {
    const character = input[cursor];
    if (STANDARD_PIECE_LETTERS.includes(character)) {
      canonical += character;
      cursor += 1;
      alternativeHasAtom = true;
      alternativeLength += 1;
      continue;
    }

    if (character === 'P') {
      const start = cursor;
      cursor += 1;
      const countStart = cursor;
      while (cursor < input.length && /[0-9]/.test(input[cursor])) cursor += 1;
      if (cursor === countStart) return null;
      const drawCount = Number(input.slice(countStart, cursor));
      if (!Number.isInteger(drawCount) || drawCount < 1 || drawCount > 7) return null;
      canonical += input.slice(start, cursor);
      pattern = true;
      alternativeHasAtom = true;
      alternativeLength += drawCount;
      continue;
    }

    if (character === '[') {
      const start = cursor;
      cursor += 1;
      const complement = input[cursor] === '^';
      if (complement) cursor += 1;
      const choicesStart = cursor;
      while (cursor < input.length && input[cursor] !== ']') {
        if (!STANDARD_PIECE_LETTERS.includes(input[cursor])) return null;
        cursor += 1;
      }
      if (cursor >= input.length || input[cursor] !== ']') return null;
      if (!complement && cursor === choicesStart) return null;
      const listed = new Set(input.slice(choicesStart, cursor));
      const choiceCount = complement ? STANDARD_PIECE_LETTERS.length - listed.size : listed.size;
      if (choiceCount < 1) return null;
      cursor += 1;

      let hasExplicitSuffix = false;
      let drawCount = 1;
      if (input[cursor] === '!') {
        cursor += 1;
        hasExplicitSuffix = true;
        drawCount = choiceCount;
      } else if (cursor < input.length && /[0-9]/.test(input[cursor])) {
        hasExplicitSuffix = true;
        const countStart = cursor;
        while (cursor < input.length && /[0-9]/.test(input[cursor])) cursor += 1;
        drawCount = Number(input.slice(countStart, cursor));
        if (!Number.isInteger(drawCount) || drawCount < 1 || drawCount > choiceCount) return null;
      }

      canonical += input.slice(start, cursor);
      if (!hasExplicitSuffix && input[cursor] === 'P') canonical += '1';
      pattern = true;
      alternativeHasAtom = true;
      alternativeLength += drawCount;
      continue;
    }

    if (character === ';') {
      if (!alternativeHasAtom) return null;
      if (sequenceLength !== null && sequenceLength !== alternativeLength) return null;
      sequenceLength = alternativeLength;
      canonical += character;
      cursor += 1;
      pattern = true;
      alternativeHasAtom = false;
      alternativeLength = 0;
      continue;
    }

    return null;
  }

  if (!alternativeHasAtom) return null;
  if (sequenceLength !== null && sequenceLength !== alternativeLength) return null;
  return {
    source: canonical,
    kind: pattern ? 'pattern' : 'fixed',
    sequenceLength: alternativeLength
  };
}

export function boardCellMask(x: number, y: number): bigint {
  return 1n << BigInt(y * 10 + x);
}

export function boardCellOccupied(mask: bigint, x: number, y: number): boolean {
  return (mask & boardCellMask(x, y)) !== 0n;
}

export function setBoardCell(mask: bigint, x: number, y: number, occupied: boolean): bigint {
  const cell = boardCellMask(x, y);
  return occupied ? mask | cell : mask & ~cell;
}

export function trimBoardMask(mask: bigint, height: number): bigint {
  const cells = Math.max(0, Math.min(64, height * 10));
  if (cells === 64) return mask & ((1n << 64n) - 1n);
  return mask & ((1n << BigInt(cells)) - 1n);
}

export type CompletedRowClear = {
  boardMask: bigint;
  clearedRows: number;
  remainingLines: number;
};

export function clearCompletedRows(mask: bigint, height: number): CompletedRowClear {
  const boundedHeight = Math.max(0, Math.min(6, Math.trunc(height)));
  const trimmed = trimBoardMask(mask, boundedHeight);
  const fullRow = (1n << 10n) - 1n;
  let boardMask = 0n;
  let writeY = 0;
  let clearedRows = 0;

  for (let readY = 0; readY < boundedHeight; readY += 1) {
    const row = (trimmed >> BigInt(readY * 10)) & fullRow;
    if (row === fullRow) {
      clearedRows += 1;
      continue;
    }
    boardMask |= row << BigInt(writeY * 10);
    writeY += 1;
  }

  return {
    boardMask,
    clearedRows,
    remainingLines: boundedHeight - clearedRows
  };
}

export function mirrorBoardMask(mask: bigint, height: number): bigint {
  let mirrored = 0n;
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < 10; x += 1) {
      if (boardCellOccupied(mask, x, y)) {
        mirrored |= boardCellMask(9 - x, y);
      }
    }
  }
  return mirrored;
}

export function boardMaskHex(mask: bigint): string {
  return `0x${mask.toString(16).padStart(16, '0')}`;
}

export function occupiedCellCount(mask: bigint): number {
  let value = mask;
  let count = 0;
  while (value !== 0n) {
    value &= value - 1n;
    count += 1;
  }
  return count;
}

export function scenarioPieceWindow(request: SolverWorkspaceRequest): number | null {
  const normalized = clearCompletedRows(request.boardMask, request.lines);
  const emptyCells = normalized.remainingLines * 10 - occupiedCellCount(normalized.boardMask);
  if (emptyCells <= 0 || emptyCells % 4 !== 0) return null;
  return emptyCells / 4;
}

export function automaticPcTargetLines(
  boardMask: bigint,
  queue: string,
  maxLines = 4
): number | null {
  const boundedMaxLines = Math.max(1, Math.min(6, Math.trunc(maxLines)));
  const parsedQueue = parseBrowserQueueInput(queue);
  if (!parsedQueue) return null;

  const normalized = clearCompletedRows(boardMask, boundedMaxLines);
  const occupiedCells = occupiedCellCount(normalized.boardMask);
  let occupiedHeight = 0;
  for (let index = 0; index < boundedMaxLines * 10; index += 1) {
    if ((normalized.boardMask & (1n << BigInt(index))) !== 0n) {
      occupiedHeight = Math.floor(index / 10) + 1;
    }
  }

  for (let lines = boundedMaxLines; lines >= Math.max(1, occupiedHeight); lines -= 1) {
    const emptyCells = lines * 10 - occupiedCells;
    if (emptyCells <= 0 || emptyCells % 4 !== 0) continue;
    if (emptyCells / 4 <= parsedQueue.sequenceLength) return lines;
  }
  return null;
}

export function workspaceValidationCodes(
  request: SolverWorkspaceRequest,
  _runtime: 'web' | 'desktop'
): WorkspaceValidationCode[] {
  const errors: WorkspaceValidationCode[] = [];
  if (!Number.isInteger(request.lines) || request.lines < 1 || request.lines > 6) {
    errors.push('target_lines_invalid');
  }
  if (!parseBrowserQueueInput(request.queue)) errors.push('queue_invalid');
  const normalized = clearCompletedRows(request.boardMask, request.lines);
  const emptyCells = normalized.remainingLines * 10 - occupiedCellCount(normalized.boardMask);
  if (emptyCells === 0) errors.push('scenario_full');
  else if (emptyCells % 4 !== 0) errors.push('scenario_not_tileable');
  if (!Number.isInteger(request.workers) || request.workers < 1) {
    errors.push('worker_count_invalid');
  }
  if (!Number.isInteger(request.initialB2B) || request.initialB2B < 0) {
    errors.push('initial_b2b_invalid');
  }
  if (request.gpuDevice !== 'auto' && !/^\d+$/.test(request.gpuDevice)) {
    errors.push('gpu_device_invalid');
  }
  return [...new Set(errors)];
}

export function buildWorkspaceCommand(request: SolverWorkspaceRequest): string {
  request = normalizeWorkspaceRequest(request);
  const tokens = [
    'clearra',
    request.scoreMode === 'failed-queue' ? 'failed-queue' : 'pc',
    '--lines',
    String(request.lines)
  ];
  const pieceWindow = scenarioPieceWindow(request);
  const parsedQueue = parseBrowserQueueInput(request.queue);
  tokens.push(
    '--board-mask',
    boardMaskHex(trimBoardMask(request.boardMask, request.lines)),
    '--height',
    String(request.lines),
    '--pieces',
    String(pieceWindow ?? 1)
  );
  if (request.scoreMode === 'tiling') tokens.push('--tiling-only');
  if (request.holdEnabled) tokens.push('--hold', request.holdPiece ?? 'empty');
  else tokens.push('--no-hold');
  if (request.queue) {
    tokens.push(
      parsedQueue?.kind === 'pattern' ? '--patterns' : '--queue',
      parsedQueue?.source ?? request.queue
    );
  }
  tokens.push(
    '--count',
    request.scoreMode === 'off' || request.scoreMode === 'tiling' ? 'unique' : 'all',
    '--backend',
    request.backend
  );
  if (request.scoreMode !== 'tiling') {
    tokens.push('--rule', request.rule);
    tokens.push(request.tablebaseEnabled ? '--tablebase' : '--no-tablebase');
    tokens.push(
      request.precomputeBuildDependencies
        ? '--build-dependency-dag'
        : '--no-build-dependency-dag'
    );
    if (request.solutionProbabilities) tokens.push('--solution-probabilities');
    tokens.push('--queue-knowledge', request.queueKnowledge);
    if (request.scoreMode === 'minimum-cover') tokens.push('--objective', 'minimum-cover');
    if (request.scoreMode === 'summary') tokens.push('--score');
    if (request.scoreMode === 'summary') {
      tokens.push('--score-profile', request.scoreProfile);
      tokens.push('--initial-b2b', String(Math.max(0, Math.trunc(request.initialB2B))));
    }
    if (request.scoreMode === 'summary' || request.preserveB2B) {
      tokens.push('--spin-profile', request.spinProfile);
    }
    if (request.preserveB2B) tokens.push('--preserve-b2b');
  }
  if (request.gpuDevice !== 'auto' && request.backend !== 'cpu') {
    tokens.push('--gpu-device', request.gpuDevice);
  }
  tokens.push(
    '--allow-backend-fallback',
    '--workers',
    String(Math.max(1, Math.trunc(request.workers))),
    '--cpu-warmup'
  );
  if (request.useAllLogicalProcessors) tokens.push('--use-all-cpu-threads');
  if (request.backend !== 'cpu') tokens.push('--gpu-warmup');
  if (request.maxPatterns !== undefined) {
    tokens.push('--max-patterns', String(Math.max(1, Math.trunc(request.maxPatterns))));
  }
  return tokens.join(' ');
}

export function workspaceRequestForDesktop(
  request: SolverWorkspaceRequest,
  language: 'en' | 'ko'
): ClearraDesktopRequest {
  request = normalizeWorkspaceRequest(request);
  const parsedQueue = parseBrowserQueueInput(request.queue);
  return {
    app_request_model: 'clearra-app/AppRequest' as const,
    command: 'pc-scenario' as const,
    language,
    lines: request.lines,
    queue: parsedQueue?.kind === 'fixed' ? parsedQueue.source : '',
    patterns: parsedQueue?.kind === 'pattern' ? parsedQueue.source : '',
    hold_enabled: request.holdEnabled,
    queue_knowledge: request.queueKnowledge,
    hold_piece: request.holdEnabled ? request.holdPiece ?? 'empty' : 'empty',
    backend: request.backend,
    rule: request.rule,
    board_mask: boardMaskHex(trimBoardMask(request.boardMask, request.lines)),
    visible_height: request.lines,
    piece_window: scenarioPieceWindow(request),
    count_policy:
      request.scoreMode === 'off' || request.scoreMode === 'tiling' ? 'unique' : 'all',
    score_mode: request.scoreMode,
    score_profile: request.scoreProfile,
    spin_profile: request.spinProfile,
    preserve_b2b: request.preserveB2B,
    precompute_build_dependencies: request.precomputeBuildDependencies,
    // The general PC/failed-queue workspace does not request finesse. Keep the
    // new desktop bridge fields explicit so it cannot inherit a prior job's
    // build-probability policy.
    finesse: 'off',
    pattern_knowledge: 'both',
    initial_b2b: Math.max(0, Math.trunc(request.initialB2B)),
    solution_probabilities: request.solutionProbabilities,
    // Desktop worker selection is automatic and must use the native host's
    // logical-processor count rather than the WebView's hardware hint.
    workers: 0,
    use_all_logical_processors: request.useAllLogicalProcessors,
    gpu_device: request.gpuDevice,
    allow_backend_fallback: true,
    memory_budget_mb: 0,
    candidate_budget: 10_000_000,
    pattern_budget: request.maxPatterns === undefined
      ? 5040
      : Math.max(1, Math.trunc(request.maxPatterns))
  };
}
