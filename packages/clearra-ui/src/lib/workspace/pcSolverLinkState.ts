import {
  normalizeQueueInput,
  trimBoardMask,
  type RuleProfile
} from './solverWorkspaceModel';
import { decodeFieldInput, encodeFieldDocument } from './fieldInterchange';

const PATH_VERSION = 'v1';
const EMPTY_QUEUE_SEGMENT = '-';
const MAX_PATH_LENGTH = 512;
const MAX_QUEUE_LENGTH = 256;
const MAX_PC_SOLVER_LINES = 4;
const RULES = new Set<RuleProfile>(['srs-plus', 'srs', 'srs-x', 'jstris-180']);

export type PcSolverLinkState = {
  lines: number;
  boardMask: bigint;
  queue: string;
  holdEnabled: boolean;
  rule: RuleProfile;
};

export const DEFAULT_PC_SOLVER_LINK_STATE: PcSolverLinkState = {
  lines: 4,
  boardMask: 0n,
  queue: '',
  holdEnabled: true,
  rule: 'srs-plus'
};

export function encodePcSolverPath(state: PcSolverLinkState): string {
  const lines = boundedLines(state.lines);
  const queue = normalizeQueueInput(state.queue);
  const queueSegment = queue ? encodeURIComponent(queue) : EMPTY_QUEUE_SEGMENT;
  return [
    PATH_VERSION,
    String(lines),
    encodeURIComponent(encodeOccupancyField(state.boardMask, MAX_PC_SOLVER_LINES)),
    state.holdEnabled ? 'hold' : 'no-hold',
    RULES.has(state.rule) ? state.rule : DEFAULT_PC_SOLVER_LINK_STATE.rule,
    queueSegment
  ].join('/');
}

export function decodePcSolverPath(path: string): PcSolverLinkState | null {
  const trimmed = path.replace(/^\/+|\/+$/g, '');
  if (!trimmed || trimmed.length > MAX_PATH_LENGTH) return null;
  const segments = trimmed.split('/');
  if (segments.length !== 6 || segments[0] !== PATH_VERSION) return null;

  const lines = Number(segments[1]);
  if (!Number.isInteger(lines) || lines < 1 || lines > MAX_PC_SOLVER_LINES) return null;
  const boardMask = decodeOccupancyField(segments[2], MAX_PC_SOLVER_LINES);
  if (boardMask === null) return null;
  const holdEnabled = segments[3] === 'hold' ? true : segments[3] === 'no-hold' ? false : null;
  if (holdEnabled === null || !RULES.has(segments[4] as RuleProfile)) return null;

  let queue = '';
  if (segments[5] !== EMPTY_QUEUE_SEGMENT) {
    try {
      queue = decodeURIComponent(segments[5]);
    } catch {
      return null;
    }
    if (queue.length > MAX_QUEUE_LENGTH || /[\u0000-\u001f\u007f]/.test(queue)) return null;
  }

  return {
    lines,
    boardMask,
    queue,
    holdEnabled,
    rule: segments[4] as RuleProfile
  };
}

function boundedLines(lines: number): number {
  return Math.max(1, Math.min(MAX_PC_SOLVER_LINES, Math.trunc(lines || 1)));
}

function encodeOccupancyField(boardMask: bigint, lines: number): string {
  const normalized = trimBoardMask(boardMask, lines);
  let height = 0;
  for (let index = 0; index < lines * 10; index += 1) {
    if ((normalized & (1n << BigInt(index))) !== 0n) {
      height = Math.floor(index / 10) + 1;
    }
  }
  const cells: Array<'G' | null> = Array.from(
    { length: height * 10 },
    (_, index) => (normalized & (1n << BigInt(index))) !== 0n ? 'G' : null
  );
  return encodeFieldDocument(
    {
      width: 10,
      pages: [{ height, cells }]
    },
    'ctk'
  );
}

function decodeOccupancyField(segment: string, lines: number): bigint | null {
  let source: string;
  try {
    source = decodeURIComponent(segment);
  } catch {
    return null;
  }
  try {
    return trimBoardMask(decodeFieldInput(source, lines).boardMask, lines);
  } catch {
    return null;
  }
}
