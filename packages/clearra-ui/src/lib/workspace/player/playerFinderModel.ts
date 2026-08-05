import type { PlayerFinderState } from "./playerEngine.ts";
import type { PlayerPiece } from "./playerRules.ts";
import {
  buildSetupFinderCommand,
  createDefaultSetupFinderRequest,
  setupFinderValidationCodes,
  type SetupFinderRequest,
} from "../setupFinderModel.ts";
import {
  automaticPcTargetLines,
  createDefaultWorkspaceRequest,
  defaultBrowserWorkerCount,
  scenarioPieceWindow,
  trimBoardMask,
  workspaceValidationCodes,
  type ScoreProfile,
  type SolverWorkspaceRequest,
} from "../solverWorkspaceModel.ts";

export const PLAYER_FINDER_PC_MAX_ROWS = 6;
export const PLAYER_FINDER_PC_DEFAULT_ROWS = 4;
// An empty hold needs one source piece beyond the 15 locks of an empty six-row PC.
export const PLAYER_FINDER_QUEUE_PIECES = 16;
export const PLAYER_FINDER_KNOWN_STATE_PIECES = 7;
export const PLAYER_FINDER_MAX_EXACT_PATTERNS = 10_000_000;

const STANDARD_PIECES = "IOTSZJL";

export type PlayerPcQueueMode = "queue-unknown" | "queue-based";

export type PlayerPcFinderOptions = Readonly<{
  hardwareConcurrency?: number;
  queueMode?: PlayerPcQueueMode;
  visibleRangeOnly?: boolean;
}>;

export type PlayerFinderSupplyPiece = Readonly<{
  piece: PlayerPiece;
  bagId: number | null;
}>;

export type PlayerPcQueuePattern = Readonly<{
  source: string;
  sequenceLength: number;
  patternCount: number;
}>;

export type PlayerFinderIssue =
  | "no-active-piece"
  | "hold-already-used"
  | "unlimited-hold-unsupported"
  | "board-above-pc-limit"
  | "no-feasible-pc-target"
  | "pc-bag-boundary-unknown"
  | "pc-residue-invalid"
  | "pc-pattern-universe-too-large"
  | "setup-board-not-empty"
  | "setup-hold-unsupported"
  | "setup-bag-boundary-unknown"
  | "setup-residue-invalid";

export type PlayerPcFinderPreparation =
  | Readonly<{
      ok: true;
      request: SolverWorkspaceRequest;
      targetLines: number;
    }>
  | Readonly<{ ok: false; issue: PlayerFinderIssue }>;

export type PlayerSetupFinderPreparation =
  | Readonly<{ ok: true; request: SetupFinderRequest }>
  | Readonly<{ ok: false; issue: PlayerFinderIssue }>;

/**
 * Builds a PC scenario from locked cells plus the active piece followed by the
 * exact generated suffix. The active placement itself is deliberately not
 * frozen: the scenario solver replans that piece from spawn.
 */
export function preparePlayerPcFinder(
  state: PlayerFinderState,
  optionsOrHardwareConcurrency: PlayerPcFinderOptions | number = {},
): PlayerPcFinderPreparation {
  if (!state.active) return { ok: false, issue: "no-active-piece" };
  if (!state.canHold) return { ok: false, issue: "hold-already-used" };
  if (state.settings.unlimitedHold) {
    return { ok: false, issue: "unlimited-hold-unsupported" };
  }
  if (occupiedHeight(state.rowMasks) > PLAYER_FINDER_PC_MAX_ROWS) {
    return { ok: false, issue: "board-above-pc-limit" };
  }

  const boardMask = rowMasksToBoardMask(state.rowMasks, PLAYER_FINDER_PC_MAX_ROWS);
  const options = normalizePcFinderOptions(optionsOrHardwareConcurrency);
  const maximumTargetRows = occupiedHeight(state.rowMasks) > PLAYER_FINDER_PC_DEFAULT_ROWS
    ? PLAYER_FINDER_PC_MAX_ROWS
    : PLAYER_FINDER_PC_DEFAULT_ROWS;
  const availableSourceCount = Math.min(
    PLAYER_FINDER_QUEUE_PIECES,
    1 + state.futureQueue.length,
  );
  const effectiveSourceCount = availableSourceCount - Number(state.hold === null);
  const targetLines = automaticPcTargetLines(
    boardMask,
    "I".repeat(Math.max(1, effectiveSourceCount)),
    maximumTargetRows,
  );
  if (targetLines === null) return { ok: false, issue: "no-feasible-pc-target" };

  const scoreProfile: ScoreProfile =
    state.settings.scoreProfile === "custom" ? "guideline" : state.settings.scoreProfile;
  const requestBase: SolverWorkspaceRequest = {
    ...createDefaultWorkspaceRequest(),
    lines: targetLines,
    boardMask: trimBoardMask(boardMask, targetLines),
    queue: "I",
    holdEnabled: true,
    holdPiece: state.hold ?? "empty",
    queueKnowledge:
      options.queueMode === "queue-based" && options.visibleRangeOnly
        ? "visible-7"
        : "oracle",
    scoreMode: "off",
    scoreProfile,
    rule: state.settings.kickProfile,
    spinProfile: state.settings.spinProfile,
    backend: "auto",
    workers: defaultBrowserWorkerCount(options.hardwareConcurrency),
    tablebaseEnabled: false,
    precomputeBuildDependencies: false,
  };
  const pieceWindow = scenarioPieceWindow(requestBase);
  if (pieceWindow === null) return { ok: false, issue: "no-feasible-pc-target" };
  const sourceWindow = pieceWindow + Number(state.hold === null);
  const sources = playerFinderSources(state).slice(0, PLAYER_FINDER_QUEUE_PIECES);
  if (sources.length < sourceWindow || sources.slice(0, sourceWindow).some(source => source.bagId === null)) {
    return { ok: false, issue: "pc-bag-boundary-unknown" };
  }
  const knownSourceCount = Math.max(
    1,
    PLAYER_FINDER_KNOWN_STATE_PIECES - Number(state.hold !== null),
  );
  const pattern = buildPlayerPcQueuePattern({
    sources,
    hold: state.hold === null
      ? null
      : Object.freeze({ piece: state.hold, bagId: state.holdBagId }),
    currentDrawBagId: state.currentDrawBagId,
    mode: options.queueMode,
    sourceWindow,
    knownSourceCount,
  });
  if (pattern === null) return { ok: false, issue: "pc-residue-invalid" };
  if (pattern.patternCount > PLAYER_FINDER_MAX_EXACT_PATTERNS) {
    return { ok: false, issue: "pc-pattern-universe-too-large" };
  }
  const request: SolverWorkspaceRequest = {
    ...requestBase,
    queue: pattern.source,
    maxPatterns: pattern.patternCount,
  };
  if (workspaceValidationCodes(request, "web").length > 0) {
    return { ok: false, issue: "no-feasible-pc-target" };
  }
  return { ok: true, request, targetLines };
}

export function buildPlayerPcQueuePattern(input: Readonly<{
  sources: readonly PlayerFinderSupplyPiece[];
  hold: PlayerFinderSupplyPiece | null;
  currentDrawBagId: number | null;
  mode: PlayerPcQueueMode;
  sourceWindow: number;
  knownSourceCount?: number;
}>): PlayerPcQueuePattern | null {
  const sourceWindow = Math.trunc(input.sourceWindow);
  if (sourceWindow < 1 || input.sources.length < sourceWindow) return null;
  if (input.sources.slice(0, sourceWindow).some(source => source.bagId === null)) return null;
  if (!validBagSegments(input.sources)) return null;

  const knownSourceCount = Math.max(
    1,
    Math.min(sourceWindow, Math.trunc(input.knownSourceCount ?? PLAYER_FINDER_KNOWN_STATE_PIECES)),
  );
  const atoms: PatternAtom[] = [];
  let cursor = 0;

  if (input.mode === "queue-based") {
    const fixed = input.sources.slice(0, knownSourceCount).map(source => source.piece).join("");
    atoms.push({ source: fixed, length: knownSourceCount, count: 1 });
    cursor = knownSourceCount;
  } else {
    while (cursor < knownSourceCount && cursor < sourceWindow) {
      const bagId = input.sources[cursor].bagId;
      const end = bagSegmentEnd(input.sources, cursor);
      const currentInventoryKnown = cursor === 0 || bagId === input.currentDrawBagId;
      const observedEnd = currentInventoryKnown
        ? Math.min(end, sourceWindow)
        : Math.min(end, knownSourceCount, sourceWindow);
      const choicesEnd = currentInventoryKnown ? end : observedEnd;
      const choices = uniquePieces(input.sources.slice(cursor, choicesEnd));
      const drawCount = observedEnd - cursor;
      const atom = explicitPermutationAtom(choices, drawCount);
      if (atom === null) return null;
      atoms.push(atom);
      cursor = observedEnd;
      if (cursor < end && cursor >= knownSourceCount) break;
    }
  }

  while (cursor < sourceWindow) {
    const atom = randomizedSuffixAtom(input, cursor, sourceWindow - cursor);
    if (atom === null) return null;
    atoms.push(atom);
    cursor += atom.length;
  }

  const source = atoms.map(atom => atom.source).join("");
  const sequenceLength = atoms.reduce((sum, atom) => sum + atom.length, 0);
  const patternCount = atoms.reduce((product, atom) => product * atom.count, 1);
  if (!source || sequenceLength !== sourceWindow || !Number.isSafeInteger(patternCount)) return null;
  return Object.freeze({ source, sequenceLength, patternCount });
}

type PatternAtom = Readonly<{ source: string; length: number; count: number }>;

function randomizedSuffixAtom(
  input: Readonly<{
    sources: readonly PlayerFinderSupplyPiece[];
    hold: PlayerFinderSupplyPiece | null;
  }>,
  cursor: number,
  remainingWindow: number,
): PatternAtom | null {
  const bagId = input.sources[cursor]?.bagId;
  if (bagId === null || bagId === undefined) return null;
  const start = bagSegmentStart(input.sources, cursor);
  const end = bagSegmentEnd(input.sources, cursor);
  const holdUsesBag = input.hold?.bagId === bagId;
  const provenFreshBag = cursor === start && start > 0 && !holdUsesBag;
  const availableInSegment = end - cursor;

  if (provenFreshBag) {
    const drawCount = Math.min(remainingWindow, availableInSegment, 7);
    return standardBagAtom(drawCount);
  }

  const choices = uniquePieces(input.sources.slice(cursor, end));
  if (choices.length !== availableInSegment || choices.length === 0) return null;
  const drawCount = Math.min(remainingWindow, choices.length);
  const excluded = exclusionOrder(input, bagId, start, cursor, choices);
  if (choices.length + excluded.length !== STANDARD_PIECES.length) {
    return explicitPermutationAtom(choices, drawCount);
  }
  const suffix = drawCount === choices.length ? "!" : String(drawCount);
  return Object.freeze({
    source: `[^${excluded.join("")}]${suffix}`,
    length: drawCount,
    count: fallingFactorial(choices.length, drawCount),
  });
}

function exclusionOrder(
  input: Readonly<{
    sources: readonly PlayerFinderSupplyPiece[];
    hold: PlayerFinderSupplyPiece | null;
  }>,
  bagId: number,
  start: number,
  cursor: number,
  remainingChoices: readonly PlayerPiece[],
): PlayerPiece[] {
  const remaining = new Set(remainingChoices);
  const excluded: PlayerPiece[] = [];
  const append = (piece: PlayerPiece) => {
    if (!remaining.has(piece) && !excluded.includes(piece)) excluded.push(piece);
  };
  for (const source of input.sources.slice(start, cursor)) append(source.piece);
  if (input.hold?.bagId === bagId) append(input.hold.piece);
  for (const piece of STANDARD_PIECES as Iterable<PlayerPiece>) append(piece);
  return excluded;
}

function explicitPermutationAtom(
  choices: readonly PlayerPiece[],
  drawCount: number,
): PatternAtom | null {
  if (drawCount < 1 || drawCount > choices.length || new Set(choices).size !== choices.length) {
    return null;
  }
  const suffix = drawCount === choices.length ? "!" : String(drawCount);
  return Object.freeze({
    source: `[${choices.join("")}]${suffix}`,
    length: drawCount,
    count: fallingFactorial(choices.length, drawCount),
  });
}

function standardBagAtom(drawCount: number): PatternAtom | null {
  if (drawCount < 1 || drawCount > 7) return null;
  return Object.freeze({
    source: `P${drawCount}`,
    length: drawCount,
    count: fallingFactorial(7, drawCount),
  });
}

function fallingFactorial(value: number, count: number): number {
  let product = 1;
  for (let offset = 0; offset < count; offset += 1) product *= value - offset;
  return product;
}

function bagSegmentStart(sources: readonly PlayerFinderSupplyPiece[], index: number): number {
  const bagId = sources[index]?.bagId;
  let start = index;
  while (start > 0 && sources[start - 1].bagId === bagId) start -= 1;
  return start;
}

function bagSegmentEnd(sources: readonly PlayerFinderSupplyPiece[], index: number): number {
  const bagId = sources[index]?.bagId;
  let end = index + 1;
  while (end < sources.length && sources[end].bagId === bagId) end += 1;
  return end;
}

function uniquePieces(sources: readonly PlayerFinderSupplyPiece[]): PlayerPiece[] {
  const pieces: PlayerPiece[] = [];
  for (const source of sources) {
    if (!pieces.includes(source.piece)) pieces.push(source.piece);
  }
  return pieces;
}

function validBagSegments(sources: readonly PlayerFinderSupplyPiece[]): boolean {
  const closed = new Set<number>();
  let current: number | null = null;
  let pieces = new Set<PlayerPiece>();
  for (const source of sources) {
    if (source.bagId === null) return false;
    if (source.bagId !== current) {
      if (current !== null) closed.add(current);
      if (closed.has(source.bagId)) return false;
      current = source.bagId;
      pieces = new Set<PlayerPiece>();
    }
    if (pieces.has(source.piece)) return false;
    pieces.add(source.piece);
  }
  return true;
}

function playerFinderSources(state: PlayerFinderState): PlayerFinderSupplyPiece[] {
  if (!state.active) return [];
  return [
    Object.freeze({ piece: state.active.piece, bagId: state.activeBagId }),
    ...state.futureQueue.map((piece, index) => Object.freeze({
      piece,
      bagId: state.futureQueueBagIds[index] ?? null,
    })),
  ];
}

function normalizePcFinderOptions(
  value: PlayerPcFinderOptions | number,
): Required<PlayerPcFinderOptions> {
  if (typeof value === "number") {
    return {
      hardwareConcurrency: value,
      queueMode: "queue-based",
      visibleRangeOnly: false,
    };
  }
  return {
    hardwareConcurrency: value.hardwareConcurrency ?? 1,
    queueMode: value.queueMode ?? "queue-based",
    visibleRangeOnly: value.visibleRangeOnly ?? false,
  };
}

/**
 * Setup search consumes an unordered standard-bag residue, not an ordered
 * NEXT string. The engine supplies that residue only when its bag boundary is
 * known. Occupied hold is intentionally rejected because the product command
 * cannot represent an arbitrary initial hold without changing its contract.
 */
export function preparePlayerSetupFinder(
  state: PlayerFinderState,
): PlayerSetupFinderPreparation {
  if (!playerFinderBoardIsEmpty(state.rowMasks)) {
    return { ok: false, issue: "setup-board-not-empty" };
  }
  if (!state.active) return { ok: false, issue: "no-active-piece" };
  if (!state.canHold) return { ok: false, issue: "hold-already-used" };
  if (state.settings.unlimitedHold) {
    return { ok: false, issue: "unlimited-hold-unsupported" };
  }
  if (state.hold !== null) return { ok: false, issue: "setup-hold-unsupported" };
  if (state.setupBagRemainder === null) {
    return { ok: false, issue: "setup-bag-boundary-unknown" };
  }

  const request: SetupFinderRequest = {
    ...createDefaultSetupFinderRequest(),
    remaining: state.setupBagRemainder.join(""),
    rule: state.settings.kickProfile,
    tablebaseEnabled: false,
    useAllLogicalProcessors: false,
  };
  if (setupFinderValidationCodes(request).length > 0) {
    return { ok: false, issue: "setup-residue-invalid" };
  }
  return { ok: true, request };
}

export function playerFinderBoardIsEmpty(rowMasks: ArrayLike<number>): boolean {
  for (let row = 0; row < rowMasks.length; row += 1) {
    if ((rowMasks[row] ?? 0) !== 0) return false;
  }
  return true;
}

export function rowMasksToBoardMask(
  rowMasks: ArrayLike<number>,
  rows: number,
): bigint {
  const boundedRows = Math.max(0, Math.min(PLAYER_FINDER_PC_MAX_ROWS, Math.trunc(rows)));
  let mask = 0n;
  for (let y = 0; y < boundedRows; y += 1) {
    mask |= BigInt((rowMasks[y] ?? 0) & 0x3ff) << BigInt(y * 10);
  }
  return mask;
}

export function occupiedHeight(rowMasks: ArrayLike<number>): number {
  for (let row = rowMasks.length - 1; row >= 0; row -= 1) {
    if ((rowMasks[row] ?? 0) !== 0) return row + 1;
  }
  return 0;
}

export function playerFinderQueueText(
  active: PlayerPiece,
  futureQueue: readonly PlayerPiece[],
): string {
  return [active, ...futureQueue].slice(0, PLAYER_FINDER_QUEUE_PIECES).join("");
}

// Keep the setup command import exercised by the same module boundary used by
// the embedded controller; this is also a narrow, pure contract seam for tests.
export function buildPlayerSetupFinderCommand(
  request: SetupFinderRequest,
  workerLimit: number,
): string {
  return buildSetupFinderCommand(request, workerLimit);
}
