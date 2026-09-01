import {
  parseSolutionKey,
  renderSolutionBoard,
  type SolutionExportBoard,
  type SolutionExportPage,
  type SolutionPiece
} from './solutionExport';

export type SolutionBoardPreviewSource =
  | 'solution-key'
  | 'replay-last-placement'
  | 'replay-final-board'
  | 'unavailable';

export type SolutionBoardPreviewView = {
  board: SolutionExportBoard | null;
  page: SolutionExportPage | null;
  source: SolutionBoardPreviewSource;
};

export type SolutionReplayPreviewStep = {
  active_piece: string;
  placement_mask: string;
  board_before_mask: string;
  board_after_placement_mask: string;
  board_after_line_clear_mask: string;
};

const PIECES = new Set<SolutionPiece>(['I', 'O', 'T', 'S', 'Z', 'J', 'L']);
const MAX_PREVIEW_HEIGHT = 24;
const BOARD_WIDTH = 10;

export function solutionBoardPreviewFromKey(
  key: string,
  targetLines: number
): SolutionBoardPreviewView {
  const page = parseSolutionKey(key);
  return page
    ? previewFromPage(page, targetLines, 'solution-key')
    : unavailablePreview();
}

/**
 * PC path products carry replay masks rather than a normalized solution key.
 * Prefer the final placement state so the active piece keeps its normal color;
 * fall back to the post-clear field when a producer cannot provide a coherent
 * placement tuple.
 */
export function solutionBoardPreviewFromReplay(
  steps: readonly SolutionReplayPreviewStep[],
  targetLines: number
): SolutionBoardPreviewView {
  const step = steps.at(-1);
  if (!step) return unavailablePreview();

  const before = parsePreviewMask(step.board_before_mask);
  const placement = parsePreviewMask(step.placement_mask);
  const afterPlacement = parsePreviewMask(step.board_after_placement_mask);
  const piece = solutionPiece(step.active_piece);
  if (
    before !== null &&
    placement !== null &&
    afterPlacement !== null &&
    piece !== null &&
    placement !== 0n &&
    popcount(placement) === 4 &&
    (before & placement) === 0n &&
    (before | placement) === afterPlacement
  ) {
    const page = pageForMasks(before | placement, targetLines, {
      initialMask: before,
      placements: [{ piece, mask: placement }]
    });
    if (page) {
      const preview = previewFromPage(page, targetLines, 'replay-last-placement');
      if (preview.board) return preview;
    }
  }

  const finalBoard = parsePreviewMask(step.board_after_line_clear_mask);
  if (finalBoard === null) return unavailablePreview();
  const page = pageForMasks(finalBoard, targetLines, {
    initialMask: finalBoard,
    placements: []
  });
  return page
    ? previewFromPage(page, targetLines, 'replay-final-board')
    : unavailablePreview();
}

function previewFromPage(
  page: SolutionExportPage,
  targetLines: number,
  source: Exclude<SolutionBoardPreviewSource, 'unavailable'>
): SolutionBoardPreviewView {
  try {
    return {
      board: renderSolutionBoard(page, normalizeTargetLines(targetLines)),
      page,
      source
    };
  } catch {
    return unavailablePreview();
  }
}

function pageForMasks(
  occupied: bigint,
  targetLines: number,
  content: Pick<SolutionExportPage, 'initialMask' | 'placements'>
): SolutionExportPage | null {
  const occupiedHeight = occupied === 0n
    ? 1
    : Math.ceil(occupied.toString(2).length / BOARD_WIDTH);
  const height = Math.max(normalizeTargetLines(targetLines), occupiedHeight);
  if (height > MAX_PREVIEW_HEIGHT) return null;
  return { height, ...content };
}

function normalizeTargetLines(value: number): number {
  if (!Number.isFinite(value)) return 1;
  return Math.max(1, Math.min(MAX_PREVIEW_HEIGHT, Math.trunc(value)));
}

function parsePreviewMask(value: string): bigint | null {
  if (!/^0x[0-9a-f]+$/iu.test(value)) return null;
  try {
    const mask = BigInt(value);
    return mask >> BigInt(MAX_PREVIEW_HEIGHT * BOARD_WIDTH) ? null : mask;
  } catch {
    return null;
  }
}

function solutionPiece(value: string): SolutionPiece | null {
  return PIECES.has(value as SolutionPiece) ? value as SolutionPiece : null;
}

function popcount(value: bigint): number {
  let count = 0;
  while (value) {
    value &= value - 1n;
    count += 1;
  }
  return count;
}

function unavailablePreview(): SolutionBoardPreviewView {
  return { board: null, page: null, source: 'unavailable' };
}
