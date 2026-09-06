import type {
  ClearraPcPathStepPayload,
  ClearraPcPathWitnessPayload
} from '../wasm/wasmCommandClient';
import type { SolutionExportPage, SolutionPiece } from './solutionExport';

export const PC_PATH_REPLAY_FRAME_DELAY_MS = 500;
export const PC_PATH_REPLAY_WIDTH = 10;

export type PcPathReplayCell = 'G' | 'I' | 'O' | 'T' | 'S' | 'Z' | 'J' | 'L' | null;

export type PcPathReplayFrame = {
  phase: 'initial' | 'lock' | 'after-clear';
  stepIndex: number | null;
  width: typeof PC_PATH_REPLAY_WIDTH;
  height: number;
  cells: PcPathReplayCell[];
};

export type PcPathCandidateGroup = {
  candidateId: string;
  representative: ClearraPcPathWitnessPayload;
  witnesses: ClearraPcPathWitnessPayload[];
  witnessCount: number;
  distinctPatternCount: number;
};

const PIECES = new Set<Exclude<PcPathReplayCell, 'G' | null>>([
  'I', 'O', 'T', 'S', 'Z', 'J', 'L'
]);
const MIN_VIEW_ROWS = 4;
const MAX_VIEW_ROWS = 24;

/**
 * The authoritative pc.path payload remains exhaustive. Presentation groups
 * only adjacent witnesses that share the App-owned canonical geometry ID and
 * selects the first witness under the payload's canonical ordering.
 */
export function groupPcPathWitnesses(
  witnesses: readonly ClearraPcPathWitnessPayload[]
): PcPathCandidateGroup[] {
  const groups: PcPathCandidateGroup[] = [];
  for (const witness of witnesses) {
    const previous = groups.at(-1);
    if (previous?.candidateId === witness.candidate_id) {
      previous.witnessCount += 1;
      previous.witnesses.push(witness);
      continue;
    }
    groups.push({
      candidateId: witness.candidate_id,
      representative: witness,
      witnesses: [witness],
      witnessCount: 1,
      distinctPatternCount: 0
    });
  }

  let witnessOffset = 0;
  for (const group of groups) {
    const patterns = new Set<string>();
    for (let index = 0; index < group.witnessCount; index += 1) {
      patterns.add(witnesses[witnessOffset + index].pattern_id);
    }
    group.distinctPatternCount = patterns.size;
    witnessOffset += group.witnessCount;
  }
  return groups;
}

/**
 * Materializes the whole selected replay geometry for copy/download. Rendering
 * may retain or display only a bounded witness slice, but export ownership is
 * the outer geometry group and must never inherit that presentation limit.
 */
export function pcPathCandidateGroupExportPages(
  group: Readonly<PcPathCandidateGroup>,
  requestedRows: number,
  expectedTerminalBoardMask: string | readonly string[] | null = null
): SolutionExportPage[] {
  const pages = group.witnesses.map((witness) => {
    let terminal: string | null;
    if (typeof expectedTerminalBoardMask === 'string' || expectedTerminalBoardMask === null) {
      terminal = expectedTerminalBoardMask;
    } else {
      // One canonical Build geometry can contain both original and mirrored
      // witnesses. Check each terminal against the query-authorized field set;
      // the representative is only the preview, not the export authority.
      terminal = witness.steps.at(-1)?.board_after_line_clear_mask ?? null;
      if (terminal === null || !expectedTerminalBoardMask.some((mask) => BigInt(mask) === BigInt(terminal!))) {
        throw new Error('A replay path terminates outside the authorized build fields.');
      }
    }
    return pcPathWitnessExportPage(witness, requestedRows, terminal);
  });
  if (pages.some((page) => page === null)) {
    throw new Error('A replay path could not be converted to a user export.');
  }
  return pages as SolutionExportPage[];
}

/**
 * Converts one typed replay witness into the existing user-facing CTK3/Fumen
 * export model. The private trace/candidate identities never need to cross the
 * clipboard boundary.
 */
export function pcPathWitnessExportPage(
  witness: ClearraPcPathWitnessPayload,
  requestedRows: number,
  expectedTerminalBoardMask: string | null = null
): SolutionExportPage | null {
  try {
    if (!Array.isArray(witness.steps) || witness.steps.length === 0) return null;
    const frames = buildPcPathReplayFrames(
      witness,
      requestedRows,
      expectedTerminalBoardMask
    );
    const logicalHeight = frames[0].height;
    const cellCount = logicalHeight * PC_PATH_REPLAY_WIDTH;
    const initialMask = parseMask(witness.steps[0].board_before_mask, cellCount);
    const logicalToDisplay = Array.from({ length: logicalHeight }, (_, row) => row);
    const placements: SolutionExportPage['placements'] = [];
    let nextDisplayRow = logicalHeight;
    let displayOccupied = initialMask;

    for (const step of witness.steps) {
      const logicalPlacement = parseMask(step.placement_mask, cellCount);
      let displayPlacement = 0n;
      forEachSetBit(logicalPlacement, cellCount, (cellIndex) => {
        const x = cellIndex % PC_PATH_REPLAY_WIDTH;
        const logicalRow = Math.floor(cellIndex / PC_PATH_REPLAY_WIDTH);
        const displayRow = logicalToDisplay[logicalRow];
        if (displayRow === undefined) {
          throw new Error('The PC path replay row history is inconsistent.');
        }
        displayPlacement |= 1n << BigInt(displayRow * PC_PATH_REPLAY_WIDTH + x);
      });
      if ((displayOccupied & displayPlacement) !== 0n) {
        throw new Error('The PC path replay display history overlaps.');
      }
      displayOccupied |= displayPlacement;
      placements.push({
        piece: replayPiece(step.active_piece) as SolutionPiece,
        mask: displayPlacement
      });

      const clearedRows = parseMask(step.cleared_row_mask, logicalHeight);
      let clearedRowCount = 0;
      for (let row = logicalHeight - 1; row >= 0; row -= 1) {
        if ((clearedRows & (1n << BigInt(row))) === 0n) continue;
        logicalToDisplay.splice(row, 1);
        clearedRowCount += 1;
      }
      for (let index = 0; index < clearedRowCount; index += 1) {
        logicalToDisplay.push(nextDisplayRow);
        nextDisplayRow += 1;
      }
    }

    const occupiedRows = displayOccupied === 0n
      ? 1
      : Math.ceil(displayOccupied.toString(2).length / PC_PATH_REPLAY_WIDTH);
    const height = Math.max(MIN_VIEW_ROWS, occupiedRows);
    if (height > MAX_VIEW_ROWS) {
      throw new Error('The PC path replay display history exceeds the export height limit.');
    }
    return { height, initialMask, placements };
  } catch {
    return null;
  }
}

/** Builds the exact visible event sequence: initial, every lock, and only real clears. */
export function buildPcPathReplayFrames(
  witness: ClearraPcPathWitnessPayload,
  requestedRows: number,
  expectedTerminalBoardMask: string | null = null
): PcPathReplayFrame[] {
  if (!Array.isArray(witness.steps) || witness.steps.length === 0) {
    throw new Error('The PC path replay has no placement steps.');
  }
  const height = replayHeight(witness.steps, requestedRows);
  const cellCount = PC_PATH_REPLAY_WIDTH * height;
  const firstBefore = parseMask(witness.steps[0].board_before_mask, cellCount);
  let cells = maskCells(firstBefore, cellCount, 'G');
  let occupied = firstBefore;
  const frames: PcPathReplayFrame[] = [frame('initial', null, height, cells)];

  for (let index = 0; index < witness.steps.length; index += 1) {
    const step = witness.steps[index];
    if (step.step_index !== String(index)) {
      throw new Error('The PC path replay step order is invalid.');
    }
    const before = parseMask(step.board_before_mask, cellCount);
    const placement = parseMask(step.placement_mask, cellCount);
    const afterPlacement = parseMask(step.board_after_placement_mask, cellCount);
    const afterClear = parseMask(step.board_after_line_clear_mask, cellCount);
    const clearedRows = parseMask(step.cleared_row_mask, height);
    const piece = replayPiece(step.active_piece);
    if (
      before !== occupied ||
      placement === 0n ||
      (before & placement) !== 0n ||
      (before | placement) !== afterPlacement
    ) {
      throw new Error('The PC path replay lock masks are inconsistent.');
    }
    forEachSetBit(placement, cellCount, (cellIndex) => {
      cells[cellIndex] = piece;
    });
    occupied = afterPlacement;
    frames.push(frame('lock', index, height, cells));

    const declaredClears = canonicalNonNegativeInteger(step.cleared_lines);
    if (
      popcount(clearedRows) !== declaredClears ||
      !clearedRowsAreFull(afterPlacement, height, clearedRows)
    ) {
      throw new Error('The PC path replay line-clear count is inconsistent.');
    }
    if (clearedRows !== 0n) {
      cells = compactClearedRows(cells, height, clearedRows);
      occupied = occupiedMask(cells);
      if (occupied !== afterClear) {
        throw new Error('The PC path replay after-clear mask is inconsistent.');
      }
      frames.push(frame('after-clear', index, height, cells));
    } else if (afterClear !== afterPlacement) {
      throw new Error('The PC path replay changed without a line clear.');
    }
  }

  const expectedTerminal = expectedTerminalBoardMask === null
    ? 0n
    : parseMask(expectedTerminalBoardMask, cellCount);
  if (occupied !== expectedTerminal) {
    throw new Error(
      expectedTerminalBoardMask === null
        ? 'The PC path replay does not end at the empty PC field.'
        : 'The Build path replay does not end at the requested terminal field.'
    );
  }
  return frames;
}

function replayHeight(steps: readonly ClearraPcPathStepPayload[], requestedRows: number): number {
  const normalizedRows = Number.isFinite(requestedRows)
    ? Math.trunc(requestedRows)
    : MIN_VIEW_ROWS;
  let occupiedRows = 1;
  for (const step of steps) {
    for (const value of [
      step.placement_mask,
      step.board_before_mask,
      step.board_after_placement_mask,
      step.board_after_line_clear_mask
    ]) {
      const mask = parseCanonicalHexMask(value);
      if (mask !== 0n) {
        occupiedRows = Math.max(
          occupiedRows,
          Math.ceil(mask.toString(2).length / PC_PATH_REPLAY_WIDTH)
        );
      }
    }
  }
  const height = Math.max(MIN_VIEW_ROWS, normalizedRows, occupiedRows);
  if (height > MAX_VIEW_ROWS) {
    throw new Error('The PC path replay exceeds the GUI height limit.');
  }
  return height;
}

function frame(
  phase: PcPathReplayFrame['phase'],
  stepIndex: number | null,
  height: number,
  cells: readonly PcPathReplayCell[]
): PcPathReplayFrame {
  return {
    phase,
    stepIndex,
    width: PC_PATH_REPLAY_WIDTH,
    height,
    cells: [...cells]
  };
}

function maskCells(mask: bigint, length: number, color: PcPathReplayCell): PcPathReplayCell[] {
  const cells = Array<PcPathReplayCell>(length).fill(null);
  forEachSetBit(mask, length, (index) => {
    cells[index] = color;
  });
  return cells;
}

function compactClearedRows(
  source: readonly PcPathReplayCell[],
  height: number,
  clearedRows: bigint
): PcPathReplayCell[] {
  const compacted = Array<PcPathReplayCell>(source.length).fill(null);
  let destinationRow = 0;
  for (let sourceRow = 0; sourceRow < height; sourceRow += 1) {
    if ((clearedRows & (1n << BigInt(sourceRow))) !== 0n) continue;
    const sourceOffset = sourceRow * PC_PATH_REPLAY_WIDTH;
    const destinationOffset = destinationRow * PC_PATH_REPLAY_WIDTH;
    for (let x = 0; x < PC_PATH_REPLAY_WIDTH; x += 1) {
      compacted[destinationOffset + x] = source[sourceOffset + x];
    }
    destinationRow += 1;
  }
  return compacted;
}

function occupiedMask(cells: readonly PcPathReplayCell[]): bigint {
  let mask = 0n;
  for (let index = 0; index < cells.length; index += 1) {
    if (cells[index] !== null) mask |= 1n << BigInt(index);
  }
  return mask;
}

function clearedRowsAreFull(board: bigint, height: number, clearedRows: bigint): boolean {
  const fullRow = (1n << BigInt(PC_PATH_REPLAY_WIDTH)) - 1n;
  for (let row = 0; row < height; row += 1) {
    if ((clearedRows & (1n << BigInt(row))) === 0n) continue;
    if (((board >> BigInt(row * PC_PATH_REPLAY_WIDTH)) & fullRow) !== fullRow) return false;
  }
  return true;
}

function parseMask(value: string, bitLimit: number): bigint {
  const mask = parseCanonicalHexMask(value);
  if ((mask >> BigInt(bitLimit)) !== 0n) {
    throw new Error('The PC path replay mask exceeds its board.');
  }
  return mask;
}

function parseCanonicalHexMask(value: string): bigint {
  if (!/^0x[0-9a-f]{16}$/u.test(value)) {
    throw new Error('The PC path replay mask is not canonical.');
  }
  return BigInt(value);
}

function replayPiece(value: string): Exclude<PcPathReplayCell, 'G' | null> {
  if (!PIECES.has(value as Exclude<PcPathReplayCell, 'G' | null>)) {
    throw new Error('The PC path replay piece is invalid.');
  }
  return value as Exclude<PcPathReplayCell, 'G' | null>;
}

function canonicalNonNegativeInteger(value: string): number {
  if (!/^(?:0|[1-9][0-9]*)$/u.test(value)) {
    throw new Error('The PC path replay count is invalid.');
  }
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed)) {
    throw new Error('The PC path replay count exceeds the GUI limit.');
  }
  return parsed;
}

function forEachSetBit(mask: bigint, limit: number, visit: (index: number) => void) {
  for (let index = 0; index < limit; index += 1) {
    if ((mask & (1n << BigInt(index))) !== 0n) visit(index);
  }
}

function popcount(mask: bigint): number {
  let count = 0;
  while (mask !== 0n) {
    mask &= mask - 1n;
    count += 1;
  }
  return count;
}
