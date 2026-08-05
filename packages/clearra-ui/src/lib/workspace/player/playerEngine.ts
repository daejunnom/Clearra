// SRP rationale: this module has one behavior-level change reason: deterministic Player simulation state transitions and their observable state contract.
import {
  PLAYER_BOARD_CELLS,
  PLAYER_BOARD_ROWS,
  PLAYER_BOARD_WIDTH,
  PLAYER_CELL_ID,
  PLAYER_FULL_ROW_MASK,
  PLAYER_PIECES,
  PLAYER_SPAWN_Y,
  PLAYER_VISIBLE_ROWS,
  playerCellIdFromCtkColor,
  playerGhostY,
  playerKickCandidates,
  playerPieceCellId,
  playerPieceOffsets,
  playerPlacementFits,
  playerRotationTarget,
  playerSpawnX,
  type PlayerBoardInputCell,
  type PlayerCellId,
  type PlayerPiece,
  type PlayerRotation,
  type PlayerRotationDirection,
} from "./playerRules.ts";
import {
  DEFAULT_PLAYER_SETTINGS,
  PLAYER_INSTANT_SDF,
  validatePlayerSettings,
  type PlayerScoreModel,
  type PlayerSettings,
  type PlayerSettingsInput,
  type PlayerSpinProfile,
} from "./playerSettings.ts";
import {
  EMPTY_PLAYER_HELD_INPUT,
  type PlayerHeldInput,
  type PlayerHorizontalPriority,
} from "./playerInput.ts";

export type PlayerStatus = "idle" | "running" | "paused" | "top-out";

export type PlayerActivePiece = Readonly<{
  piece: PlayerPiece;
  rotation: PlayerRotation;
  x: number;
  y: number;
}>;

export type PlayerSpinKind = "t-spin" | "t-spin-mini" | "all-spin" | "all-spin-mini";

export type PlayerSpinInfo = Readonly<{
  kind: PlayerSpinKind;
  piece: PlayerPiece;
  mini: boolean;
  profile: PlayerSpinProfile;
  rotation: PlayerRotation;
  kickIndex: number;
}>;

export type PlayerClearInfo = Readonly<{
  lines: number;
  spin: PlayerSpinInfo | null;
  perfectClear: boolean;
  difficult: boolean;
  combo: number;
  comboIndex: number;
  backToBackApplied: boolean;
  backToBackChain: number;
  scoreAward: number;
}>;

export type PlayerAction = Readonly<{
  type:
    | "move-left"
    | "move-right"
    | "soft-drop"
    | "hard-drop"
    | "rotate-cw"
    | "rotate-ccw"
    | "rotate-180"
    | "hold"
    | "start"
    | "pause"
    | "toggle-pause"
    | "reset"
    | "undo"
    | "redo";
}>;

export type PlayerActionResult = Readonly<{
  changed: boolean;
  locked: boolean;
  linesCleared: number;
  topOut: boolean;
  revision: number;
}>;

export type PlayerAdvanceResult = PlayerActionResult &
  Readonly<{
    steps: number;
    droppedMs: number;
  }>;

export type PlayerRenderView = Readonly<{
  board: Uint8Array;
  rowMasks: Uint16Array;
  active: PlayerActivePiece | null;
  ghostY: number | null;
  hold: PlayerPiece | null;
  queue: readonly PlayerPiece[];
  status: PlayerStatus;
  revision: number;
  linesCleared: number;
  piecesLocked: number;
  lastClear: number;
  canHold: boolean;
  lockElapsedMs: number;
  lockResetCount: number;
  elapsedMs: number;
  score: number;
  combo: number;
  backToBackChain: number;
  lastSpin: PlayerSpinInfo | null;
  lastClearInfo: PlayerClearInfo | null;
  canUndo: boolean;
  canRedo: boolean;
}>;

export type PlayerSnapshot = Readonly<{
  board: Uint8Array;
  rowMasks: Uint16Array;
  active: PlayerActivePiece | null;
  ghostY: number | null;
  hold: PlayerPiece | null;
  queue: readonly PlayerPiece[];
  status: PlayerStatus;
  revision: number;
  seed: number;
  randomState: number;
  settings: PlayerSettings;
  linesCleared: number;
  piecesLocked: number;
  lastClear: number;
  canHold: boolean;
  lockElapsedMs: number;
  lockResetCount: number;
  elapsedMs: number;
  score: number;
  combo: number;
  backToBackChain: number;
  lastSpin: PlayerSpinInfo | null;
  lastClearInfo: PlayerClearInfo | null;
}>;

/** Immutable, search-only supply view. Unlike the render queue, this includes
 * enough generated suffix pieces for an empty six-row PC scenario. */
export type PlayerFinderState = Readonly<{
  board: Uint8Array;
  rowMasks: Uint16Array;
  active: PlayerActivePiece | null;
  /** Opaque standard-bag provenance for the active source piece. */
  activeBagId: number | null;
  hold: PlayerPiece | null;
  /** Opaque standard-bag provenance for the held source piece. */
  holdBagId: number | null;
  canHold: boolean;
  futureQueue: readonly PlayerPiece[];
  /** Bag provenance aligned one-to-one with futureQueue. */
  futureQueueBagIds: readonly (number | null)[];
  /** Last bag consumed from the draw stream; hold swaps do not change it. */
  currentDrawBagId: number | null;
  /** Active piece plus the unconsumed suffix of its known standard bag. */
  setupBagRemainder: readonly PlayerPiece[] | null;
  settings: PlayerSettings;
  revision: number;
}>;

export type PlayerEngineOptions = Readonly<{
  settings?: PlayerSettingsInput;
  seed?: number | string;
  initialBoard?: ArrayLike<PlayerBoardInputCell>;
  initialQueue?: readonly PlayerPiece[];
  autoStart?: boolean;
}>;

export interface PlayerEngine {
  readonly revision: number;
  readonly status: PlayerStatus;
  readonly settings: PlayerSettings;
  readonly seed: number;
  advance(deltaMs: number, heldInput?: PlayerHeldInput): PlayerAdvanceResult;
  dispatch(action: PlayerAction | PlayerAction["type"]): PlayerActionResult;
  reset(): PlayerActionResult;
  start(): PlayerActionResult;
  pause(): PlayerActionResult;
  togglePause(): PlayerActionResult;
  undo(): PlayerActionResult;
  redo(): PlayerActionResult;
  updateSettings(settings: PlayerSettingsInput): PlayerSettings;
  loadBoard(cells: ArrayLike<PlayerBoardInputCell>): PlayerActionResult;
  loadQueue(pieces: readonly PlayerPiece[]): PlayerActionResult;
  getRenderView(): PlayerRenderView;
  getFinderState(): PlayerFinderState;
  snapshot(): PlayerSnapshot;
  subscribe(listener: (view: PlayerRenderView) => void, emitCurrent?: boolean): () => void;
}

type MutableRenderView = {
  board: Uint8Array;
  rowMasks: Uint16Array;
  active: PlayerActivePiece | null;
  ghostY: number | null;
  hold: PlayerPiece | null;
  queue: readonly PlayerPiece[];
  status: PlayerStatus;
  revision: number;
  linesCleared: number;
  piecesLocked: number;
  lastClear: number;
  canHold: boolean;
  lockElapsedMs: number;
  lockResetCount: number;
  elapsedMs: number;
  score: number;
  combo: number;
  backToBackChain: number;
  lastSpin: PlayerSpinInfo | null;
  lastClearInfo: PlayerClearInfo | null;
  canUndo: boolean;
  canRedo: boolean;
};

type Mutation = {
  changed: boolean;
  locked: boolean;
  linesCleared: number;
  topOut: boolean;
  historyDiverging?: boolean;
};

type PlayerRotationEvidence = Readonly<{
  direction: PlayerRotationDirection;
  kickIndex: number;
}>;

type PlayerHistoryState = Readonly<{
  board: Uint8Array;
  active: PlayerActivePiece | null;
  activeBagId: number | null;
  activeWasClutchSpawned: boolean;
  status: PlayerStatus;
  hold: PlayerPiece | null;
  holdBagId: number | null;
  queue: readonly PlayerPiece[];
  queueBagIds: readonly (number | null)[];
  currentDrawBagId: number | null;
  nextRandomBagId: number;
  initialQueueIndex: number;
  randomState: number;
  holdUsedThisTurn: boolean;
  linesCleared: number;
  piecesLocked: number;
  lastClear: number;
  dropAccumulator: number;
  lockResetCount: number;
  elapsedMs: number;
  score: number;
  pieceSpawnScore: number;
  combo: number;
  backToBackChain: number;
  lastSpin: PlayerSpinInfo | null;
  lastClearInfo: PlayerClearInfo | null;
  lastRotation: PlayerRotationEvidence | null;
}>;

type PlayerLockHistoryEntry = Readonly<{
  before: PlayerHistoryState;
  after: PlayerHistoryState;
}>;

export function createPlayerEngine(options: PlayerEngineOptions = {}): PlayerEngine {
  const board = new Uint8Array(PLAYER_BOARD_CELLS);
  const rowMasks = new Uint16Array(PLAYER_BOARD_ROWS);
  const initialBoard = new Uint8Array(PLAYER_BOARD_CELLS);
  const queue: PlayerPiece[] = [];
  const queueBagIds: Array<number | null> = [];
  const queueView: PlayerPiece[] = [];
  const listeners = new Set<(view: PlayerRenderView) => void>();
  const history: PlayerLockHistoryEntry[] = [];
  let initialQueue = normalizeInitialQueue(options.initialQueue);
  const seed = normalizeSeed(options.seed ?? Date.now());

  let settings = validatePlayerSettings(options.settings ?? {}, DEFAULT_PLAYER_SETTINGS);
  let randomState = seed;
  let randomSessionIndex = 0;
  let firstRandomBagPending = false;
  let previousFirstRandomBag: readonly PlayerPiece[] | null = null;
  let queueHead = 0;
  let initialQueueIndex = 0;
  let currentDrawBagId: number | null = null;
  let nextRandomBagId = 1;
  let historyCursor = 0;
  let active: PlayerActivePiece | null = null;
  let activeBagId: number | null = null;
  let activeWasClutchSpawned = false;
  let ghostY: number | null = null;
  let hold: PlayerPiece | null = null;
  let holdBagId: number | null = null;
  let holdUsedThisTurn = false;
  let canHold = true;
  let status: PlayerStatus = "idle";
  let revision = 0;
  let linesCleared = 0;
  let piecesLocked = 0;
  let lastClear = 0;
  let stepAccumulatorMs = 0;
  let dropAccumulator = 0;
  let lockElapsedMs = 0;
  let lockResetCount = 0;
  let elapsedMs = 0;
  let score = 0;
  let pieceSpawnScore = 0;
  let combo = 0;
  let backToBackChain = 0;
  let lastSpin: PlayerSpinInfo | null = null;
  let lastClearInfo: PlayerClearInfo | null = null;
  let lastRotation: PlayerRotationEvidence | null = null;
  let horizontalDirection: -1 | 0 | 1 = 0;
  let horizontalHeldMs = 0;
  let arrAccumulatorMs = 0;
  let instantArrApplied = false;
  let previousSoftDrop = false;

  if (options.initialBoard) writeBoard(initialBoard, options.initialBoard);
  board.set(initialBoard);
  rebuildRowMasks(board, rowMasks);

  const renderView: MutableRenderView = {
    board,
    rowMasks,
    active,
    ghostY,
    hold,
    queue: queueView,
    status,
    revision,
    linesCleared,
    piecesLocked,
    lastClear,
    canHold,
    lockElapsedMs,
    lockResetCount,
    elapsedMs,
    score,
    combo,
    backToBackChain,
    lastSpin,
    lastClearInfo,
    canUndo: false,
    canRedo: false,
  };

  const engine: PlayerEngine = {
    get revision() {
      return revision;
    },
    get status() {
      return status;
    },
    get settings() {
      return settings;
    },
    get seed() {
      return seed;
    },
    advance(deltaMs, heldInput = EMPTY_PLAYER_HELD_INPUT) {
      assertDelta(deltaMs);
      const mutation = emptyMutation();
      if (status !== "running" || !active) {
        return advanceResult(mutation, 0, 0);
      }
      const held = normalizeHeldInput(heldInput);
      const maximumBuffered = settings.fixedStepMs * settings.maxCatchUpSteps;
      const totalBuffered = stepAccumulatorMs + deltaMs;
      const droppedMs = Math.max(0, totalBuffered - maximumBuffered);
      stepAccumulatorMs = Math.min(totalBuffered, maximumBuffered);
      let steps = 0;
      while (
        steps < settings.maxCatchUpSteps &&
        stepAccumulatorMs + Number.EPSILON >= settings.fixedStepMs &&
        status === "running" &&
        active
      ) {
        stepAccumulatorMs -= settings.fixedStepMs;
        mergeMutation(mutation, simulateStep(settings.fixedStepMs, held));
        steps += 1;
      }
      if (status !== "running") stepAccumulatorMs = 0;
      if (mutation.changed) {
        if (mutation.historyDiverging) discardRedoTail();
        publish();
      }
      return advanceResult(mutation, steps, droppedMs);
    },
    dispatch(action) {
      const type = typeof action === "string" ? action : action?.type;
      const mutation = dispatchAction(type);
      if (mutation.changed) {
        if (isGameplayAction(type)) discardRedoTail();
        publish();
      }
      return actionResult(mutation);
    },
    reset() {
      const mutation = resetCore();
      publish();
      return actionResult(mutation);
    },
    start() {
      const mutation = startCore();
      if (mutation.changed) publish();
      return actionResult(mutation);
    },
    pause() {
      const mutation = pauseCore();
      if (mutation.changed) publish();
      return actionResult(mutation);
    },
    togglePause() {
      const mutation = status === "paused" ? startCore() : pauseCore();
      if (mutation.changed) publish();
      return actionResult(mutation);
    },
    undo() {
      const mutation = undoCore();
      if (mutation.changed) publish();
      return actionResult(mutation);
    },
    redo() {
      const mutation = redoCore();
      if (mutation.changed) publish();
      return actionResult(mutation);
    },
    updateSettings(input) {
      const nextSettings = validatePlayerSettings(input, settings);
      if (playerSettingsEqual(settings, nextSettings)) return settings;
      settings = nextSettings;
      canHold = isUnlimitedHoldEnabled() || !holdUsedThisTurn;
      stepAccumulatorMs = Math.min(
        stepAccumulatorMs,
        settings.fixedStepMs * settings.maxCatchUpSteps,
      );
      ensureQueue();
      refreshQueueView();
      publish();
      return settings;
    },
    loadBoard(cells) {
      writeBoard(initialBoard, cells);
      const mutation = resetCore();
      publish();
      return actionResult(mutation);
    },
    loadQueue(pieces) {
      const nextQueue = normalizeInitialQueue(pieces);
      initialQueue = nextQueue;
      const mutation = resetCore();
      publish();
      return actionResult(mutation);
    },
    getRenderView() {
      return renderView;
    },
    getFinderState() {
      // Empty six-row PC with an empty hold needs active + 15 future source
      // pieces. Generating the suffix early does not change its deterministic
      // order and avoids coupling search correctness to previewCount.
      ensureQueue(15);
      const futureQueue = Object.freeze(queue.slice(queueHead, queueHead + 15));
      const futureQueueBagIds = Object.freeze(
        queueBagIds.slice(queueHead, queueHead + futureQueue.length),
      );
      const setupBagRemainder = currentSetupBagRemainder();
      return Object.freeze({
        board: board.slice(),
        rowMasks: rowMasks.slice(),
        active,
        activeBagId,
        hold,
        holdBagId,
        canHold,
        futureQueue,
        futureQueueBagIds,
        currentDrawBagId,
        setupBagRemainder,
        settings,
        revision,
      });
    },
    snapshot() {
      return Object.freeze({
        board: board.slice(),
        rowMasks: rowMasks.slice(),
        active,
        ghostY,
        hold,
        queue: Object.freeze(queueView.slice()),
        status,
        revision,
        seed,
        randomState,
        settings,
        linesCleared,
        piecesLocked,
        lastClear,
        canHold,
        lockElapsedMs,
        lockResetCount,
        elapsedMs,
        score,
        combo,
        backToBackChain,
        lastSpin,
        lastClearInfo,
      });
    },
    subscribe(listener, emitCurrent = true) {
      if (typeof listener !== "function") throw new TypeError("Player listener must be a function.");
      listeners.add(listener);
      if (emitCurrent) listener(renderView);
      return () => listeners.delete(listener);
    },
  };

  if (options.autoStart !== false) {
    resetCore();
    publish();
  } else {
    syncRenderView();
  }

  return engine;

  function dispatchAction(type: PlayerAction["type"] | undefined): Mutation {
    if (type === "reset") return resetCore();
    if (type === "start") return startCore();
    if (type === "pause") return pauseCore();
    if (type === "toggle-pause") return status === "paused" ? startCore() : pauseCore();
    if (type === "undo") return undoCore();
    if (type === "redo") return redoCore();
    if (status !== "running" || !active) return emptyMutation();

    if (type === "move-left") return mutationFromChanged(tryMoveHorizontal(-1));
    if (type === "move-right") return mutationFromChanged(tryMoveHorizontal(1));
    if (type === "soft-drop") return mutationFromChanged(tryMoveDown(true));
    if (type === "rotate-cw") return mutationFromChanged(tryRotate("cw"));
    if (type === "rotate-ccw") return mutationFromChanged(tryRotate("ccw"));
    if (type === "rotate-180") return mutationFromChanged(tryRotate("180"));
    if (type === "hold") return holdPiece();
    if (type === "hard-drop") return hardDrop();
    throw new RangeError(`Unsupported player action '${String(type)}'.`);
  }

  function simulateStep(deltaMs: number, held: PlayerHeldInput): Mutation {
    const mutation = emptyMutation();
    elapsedMs += deltaMs;
    // Elapsed time is HUD-only state. Keep the stable view current without
    // invalidating the board canvas or notifying render subscribers.
    renderView.elapsedMs = elapsedMs;
    mergeMutation(mutation, processHorizontal(deltaMs, held));
    if (status !== "running" || !active) return mutation;
    const wasSoftDropping = previousSoftDrop;
    previousSoftDrop = held.softDrop;
    if (held.softDrop && !wasSoftDropping && settings.sdf < PLAYER_INSTANT_SDF) {
      markHistoryDivergingChange(mutation, tryMoveDown(true));
    }
    const gravityChanged = applyGravity(deltaMs, held.softDrop);
    mutation.changed = gravityChanged || mutation.changed;
    if (gravityChanged && held.softDrop) mutation.historyDiverging = true;
    if (!active) return mutation;

    // Gravity 0 is the explicit free-placement/practice mode. The piece may
    // still be moved down manually, and hard drop still locks immediately,
    // but elapsed simulation time must never force a placement in this mode.
    if (settings.gravityG <= 0) {
      if (lockElapsedMs !== 0) {
        lockElapsedMs = 0;
        mutation.changed = true;
      }
    } else if (isGrounded(active)) {
      const before = lockElapsedMs;
      lockElapsedMs = Math.min(settings.lockDelayMs, lockElapsedMs + deltaMs);
      mutation.changed = lockElapsedMs !== before || mutation.changed;
      if (lockElapsedMs + Number.EPSILON >= settings.lockDelayMs) {
        mergeMutation(mutation, lockActive());
      }
    } else if (lockElapsedMs !== 0) {
      lockElapsedMs = 0;
      mutation.changed = true;
    }
    return mutation;
  }

  function processHorizontal(deltaMs: number, held: PlayerHeldInput): Mutation {
    const mutation = emptyMutation();
    const direction = resolveHorizontalDirection(held, horizontalDirection);
    if (direction !== horizontalDirection) {
      horizontalDirection = direction;
      horizontalHeldMs = 0;
      arrAccumulatorMs = 0;
      instantArrApplied = false;
      if (direction !== 0) {
        markHistoryDivergingChange(mutation, tryMoveHorizontal(direction));
        if (settings.dasMs === 0 && settings.arrMs === 0) {
          instantArrApplied = true;
          for (let moves = 0; moves < PLAYER_BOARD_WIDTH; moves += 1) {
            if (!tryMoveHorizontal(direction)) break;
            markHistoryDivergingChange(mutation, true);
          }
        }
      }
      return mutation;
    }
    if (direction === 0) return mutation;

    const previousHeldMs = horizontalHeldMs;
    horizontalHeldMs += deltaMs;
    if (horizontalHeldMs + Number.EPSILON < settings.dasMs) return mutation;

    if (settings.arrMs === 0) {
      if (!instantArrApplied) {
        instantArrApplied = true;
        for (let moves = 0; moves < PLAYER_BOARD_WIDTH; moves += 1) {
          if (!tryMoveHorizontal(direction)) break;
          markHistoryDivergingChange(mutation, true);
        }
      }
      return mutation;
    }

    if (previousHeldMs < settings.dasMs) {
      markHistoryDivergingChange(mutation, tryMoveHorizontal(direction));
      arrAccumulatorMs = Math.max(0, horizontalHeldMs - settings.dasMs);
    } else {
      arrAccumulatorMs += deltaMs;
    }
    for (let moves = 0; moves < PLAYER_BOARD_WIDTH; moves += 1) {
      if (arrAccumulatorMs + Number.EPSILON < settings.arrMs) break;
      arrAccumulatorMs -= settings.arrMs;
      if (!tryMoveHorizontal(direction)) {
        arrAccumulatorMs = 0;
        break;
      }
      markHistoryDivergingChange(mutation, true);
    }
    return mutation;
  }

  function applyGravity(deltaMs: number, softDrop: boolean): boolean {
    if (!active) return false;
    if (softDrop && settings.sdf >= PLAYER_INSTANT_SDF) {
      if (ghostY === null || ghostY === active.y) return false;
      const distance = active.y - ghostY;
      active = freezeActive(active.piece, active.rotation, active.x, ghostY);
      lastRotation = null;
      instantArrApplied = false;
      addScore(distance * settings.scoreModel.softDropScorePerCell);
      refreshGhost();
      dropAccumulator = 0;
      return true;
    }
    const gravityG = softDrop ? Math.max(settings.gravityG, settings.sdf) : settings.gravityG;
    if (gravityG <= 0) return false;
    dropAccumulator += gravityG * (deltaMs / BASE_FRAME_MS);
    let changed = false;
    const cells = Math.min(PLAYER_BOARD_ROWS, Math.floor(dropAccumulator));
    if (cells > 0) dropAccumulator -= cells;
    for (let index = 0; index < cells; index += 1) {
      if (!tryMoveDown(softDrop)) {
        dropAccumulator = 0;
        break;
      }
      changed = true;
    }
    return changed;
  }

  function tryMoveHorizontal(direction: -1 | 1): boolean {
    if (!active) return false;
    const candidate = freezeActive(
      active.piece,
      active.rotation,
      active.x + direction,
      active.y,
    );
    const changed = commitPlayerTransform(candidate);
    if (changed) lastRotation = null;
    return changed;
  }

  function tryMoveDown(scoreSoftDrop = false): boolean {
    if (!active) return false;
    const candidate = freezeActive(active.piece, active.rotation, active.x, active.y - 1);
    if (!fits(candidate)) return false;
    active = candidate;
    lastRotation = null;
    instantArrApplied = false;
    if (scoreSoftDrop) addScore(settings.scoreModel.softDropScorePerCell);
    refreshGhost();
    return true;
  }

  function tryRotate(direction: PlayerRotationDirection): boolean {
    if (!active) return false;
    const target = playerRotationTarget(active.rotation, direction);
    const kicks = playerKickCandidates(
      active.piece,
      active.rotation,
      target,
      settings.kickProfile,
    );
    for (let kickIndex = 0; kickIndex < kicks.length; kickIndex += 1) {
      const kick = kicks[kickIndex];
      const candidate = freezeActive(
        active.piece,
        target,
        active.x + kick.dx,
        active.y + kick.dy,
      );
      if (commitPlayerTransform(candidate)) {
        lastRotation = Object.freeze({ direction, kickIndex });
        instantArrApplied = false;
        return true;
      }
    }
    return false;
  }

  function commitPlayerTransform(candidate: PlayerActivePiece): boolean {
    if (!active || !fits(candidate)) return false;
    const groundedBefore = isGrounded(active);
    active = candidate;
    refreshGhost();
    const resetLimit = Math.min(PLAYER_LOCK_RESET_HARD_LIMIT, settings.lockResetLimit);
    if (groundedBefore && lockResetCount < resetLimit) {
      lockResetCount += 1;
      lockElapsedMs = 0;
    }
    return true;
  }

  function hardDrop(): Mutation {
    if (!active) return emptyMutation();
    if (ghostY !== null && ghostY !== active.y) {
      const distance = active.y - ghostY;
      active = freezeActive(active.piece, active.rotation, active.x, ghostY);
      lastRotation = null;
      addScore(distance * settings.scoreModel.hardDropScorePerCell);
    }
    return lockActive();
  }

  function holdPiece(): Mutation {
    if (!active || !canHold) return emptyMutation();
    const outgoing = Object.freeze({ piece: active.piece, bagId: activeBagId });
    const incoming = hold === null
      ? dequeuePiece()
      : Object.freeze({ piece: hold, bagId: holdBagId });
    hold = outgoing.piece;
    holdBagId = outgoing.bagId;
    holdUsedThisTurn = true;
    canHold = isUnlimitedHoldEnabled();
    resetPieceTiming();
    const topOut = !spawnPiece(incoming);
    return { changed: true, locked: false, linesCleared: 0, topOut };
  }

  function lockActive(): Mutation {
    if (!active) return emptyMutation();
    const beforeLock = captureHistoryState();
    const lockingPiece = active;
    const spin = classifySpin(lockingPiece);
    const color = playerPieceCellId(active.piece);
    for (const offset of playerPieceOffsets(active.piece, active.rotation)) {
      const x = active.x + offset.x;
      const y = active.y + offset.y;
      board[y * PLAYER_BOARD_WIDTH + x] = color;
      rowMasks[y] |= 1 << x;
    }
    piecesLocked += 1;
    const cleared = clearFullRows();
    const perfectClear = cleared > 0 && !hasAnyOccupancy();
    lastSpin = spin;
    lastClearInfo = scoreLock(cleared, spin, perfectClear);
    linesCleared += cleared;
    lastClear = cleared;
    active = null;
    ghostY = null;
    holdUsedThisTurn = false;
    canHold = true;
    resetPieceTiming();

    // Lock-out is determined solely by whether the next piece can spawn.
    // Occupancy above the visible playfield is valid internal simulation state.
    const topOut = !spawnPiece(dequeuePiece(), cleared > 0);
    recordLock(beforeLock);
    return { changed: true, locked: true, linesCleared: cleared, topOut };
  }

  function clearFullRows(): number {
    let writeRow = 0;
    let cleared = 0;
    for (let readRow = 0; readRow < PLAYER_BOARD_ROWS; readRow += 1) {
      if (rowMasks[readRow] === PLAYER_FULL_ROW_MASK) {
        cleared += 1;
        continue;
      }
      if (writeRow !== readRow) {
        board.copyWithin(
          writeRow * PLAYER_BOARD_WIDTH,
          readRow * PLAYER_BOARD_WIDTH,
          (readRow + 1) * PLAYER_BOARD_WIDTH,
        );
        rowMasks[writeRow] = rowMasks[readRow];
      }
      writeRow += 1;
    }
    if (cleared > 0) {
      board.fill(PLAYER_CELL_ID.empty, writeRow * PLAYER_BOARD_WIDTH);
      rowMasks.fill(0, writeRow);
    }
    return cleared;
  }

  function classifySpin(candidate: PlayerActivePiece): PlayerSpinInfo | null {
    const evidence = lastRotation;
    if (!evidence) return null;

    if (candidate.piece === "T") {
      const [centerX, centerY] = tCenter(candidate);
      const blockedCorners = T_CORNER_OFFSETS.reduce(
        (count, [dx, dy]) =>
          count + Number(spinCellBlocked(candidate, centerX + dx, centerY + dy)),
        0,
      );
      if (blockedCorners >= 3) {
        const blockedFront = tFrontOffsets(candidate.rotation).reduce(
          (count, [dx, dy]) =>
            count + Number(spinCellBlocked(candidate, centerX + dx, centerY + dy)),
          0,
        );
        const finalQuarterTurnKick =
          evidence.direction !== "180" && evidence.kickIndex === 4;
        const mini = blockedFront < 2 && !finalQuarterTurnKick;
        return freezeSpin(
          mini ? "t-spin-mini" : "t-spin",
          candidate,
          mini,
          evidence.kickIndex,
        );
      }
      if (isPlusSpinProfile(settings.spinProfile) && isImmobile(candidate)) {
        return freezeSpin("t-spin-mini", candidate, true, evidence.kickIndex);
      }
      return null;
    }

    if (!isImmobile(candidate)) return null;
    if (settings.spinProfile === "all-spin" || settings.spinProfile === "all-spin-plus") {
      return freezeSpin("all-spin", candidate, false, evidence.kickIndex);
    }
    if (settings.spinProfile === "all-mini" || settings.spinProfile === "all-mini-plus") {
      return freezeSpin("all-spin-mini", candidate, true, evidence.kickIndex);
    }
    return null;
  }

  function freezeSpin(
    kind: PlayerSpinKind,
    candidate: PlayerActivePiece,
    mini: boolean,
    kickIndex: number,
  ): PlayerSpinInfo {
    return Object.freeze({
      kind,
      piece: candidate.piece,
      mini,
      profile: settings.spinProfile,
      rotation: candidate.rotation,
      kickIndex,
    });
  }

  function spinCellBlocked(
    candidate: PlayerActivePiece,
    x: number,
    y: number,
  ): boolean {
    if (x < 0 || x >= PLAYER_BOARD_WIDTH || y < 0) return true;
    if (y >= PLAYER_BOARD_ROWS) return false;
    if ((rowMasks[y] & (1 << x)) !== 0) return true;
    return playerPieceOffsets(candidate.piece, candidate.rotation).some(
      (offset) => candidate.x + offset.x === x && candidate.y + offset.y === y,
    );
  }

  function isImmobile(candidate: PlayerActivePiece): boolean {
    return (
      !playerPlacementFits(
        rowMasks,
        candidate.piece,
        candidate.rotation,
        candidate.x - 1,
        candidate.y,
      ) &&
      !playerPlacementFits(
        rowMasks,
        candidate.piece,
        candidate.rotation,
        candidate.x + 1,
        candidate.y,
      ) &&
      !playerPlacementFits(
        rowMasks,
        candidate.piece,
        candidate.rotation,
        candidate.x,
        candidate.y - 1,
      ) &&
      !playerPlacementFits(
        rowMasks,
        candidate.piece,
        candidate.rotation,
        candidate.x,
        candidate.y + 1,
      )
    );
  }

  function scoreLock(
    cleared: number,
    spin: PlayerSpinInfo | null,
    perfectClear: boolean,
  ): PlayerClearInfo {
    const model = settings.scoreModel;
    const b2bBefore = backToBackChain;
    combo = cleared === 0 ? 0 : Math.min(Number.MAX_SAFE_INTEGER, combo + 1);
    const difficult = cleared === 4 || (cleared > 0 && spin !== null);
    const backToBackApplied = difficult && b2bBefore > 0;
    let actionScore = actionScoreFor(model, cleared, spin);
    if (perfectClear && model.perfectClearMode === "replace-action") {
      actionScore = model.perfectClearBonuses[Math.min(4, cleared)];
      if (backToBackApplied) {
        actionScore = boundedScore(actionScore * model.backToBackMultiplier);
      }
    } else if (backToBackApplied) {
      actionScore = boundedScore(actionScore * model.backToBackMultiplier);
    }
    const comboBonus =
      cleared === 0 ? 0 : boundedScore((combo - 1) * model.comboBonusPerStep);
    const perfectClearBonus = perfectClear && model.perfectClearMode === "additive"
      ? cleared === 4 && b2bBefore > 0
        ? model.backToBackTetrisPerfectClearBonus
        : model.perfectClearBonuses[Math.min(4, cleared)]
      : 0;
    const scoreAward = boundedScore(actionScore + comboBonus + perfectClearBonus);
    addScore(scoreAward);

    if (cleared > 0) {
      backToBackChain = difficult
        ? Math.min(Number.MAX_SAFE_INTEGER, b2bBefore + 1)
        : 0;
    }

    return Object.freeze({
      lines: cleared,
      spin,
      perfectClear,
      difficult,
      combo,
      comboIndex: cleared === 0 ? -1 : combo - 1,
      backToBackApplied,
      backToBackChain,
      scoreAward,
    });
  }

  function addScore(value: number) {
    score = boundedScore(score + value);
  }

  function hasAnyOccupancy(): boolean {
    for (let row = 0; row < PLAYER_BOARD_ROWS; row += 1) {
      if (rowMasks[row] !== 0) return true;
    }
    return false;
  }

  function spawnPiece(
    source: Readonly<{ piece: PlayerPiece; bagId: number | null }>,
    clutchClearEligible = false,
  ): boolean {
    const { piece, bagId } = source;
    const standard = freezeActive(piece, "spawn", playerSpawnX(piece), PLAYER_SPAWN_Y);
    const candidate = fits(standard)
      ? standard
      : clutchClearEligible && isClutchClearEnabled()
        ? highestHiddenSpawn(piece)
        : null;
    if (!candidate) {
      active = null;
      activeBagId = null;
      activeWasClutchSpawned = false;
      ghostY = null;
      status = "top-out";
      return false;
    }
    active = candidate;
    activeBagId = bagId;
    activeWasClutchSpawned = candidate !== standard;
    status = "running";
    pieceSpawnScore = score;
    resetPieceTiming();
    refreshGhost();
    ensureQueue();
    refreshQueueView();
    return true;
  }

  function highestHiddenSpawn(piece: PlayerPiece): PlayerActivePiece | null {
    const x = playerSpawnX(piece);
    for (let y = PLAYER_BOARD_ROWS - 1; y > PLAYER_SPAWN_Y; y -= 1) {
      const candidate = freezeActive(piece, "spawn", x, y);
      if (fits(candidate)) return candidate;
    }
    return null;
  }

  function fits(candidate: PlayerActivePiece): boolean {
    return playerPlacementFits(
      rowMasks,
      candidate.piece,
      candidate.rotation,
      candidate.x,
      candidate.y,
    );
  }

  function isGrounded(candidate: PlayerActivePiece): boolean {
    return !playerPlacementFits(
      rowMasks,
      candidate.piece,
      candidate.rotation,
      candidate.x,
      candidate.y - 1,
    );
  }

  function refreshGhost() {
    ghostY = active
      ? playerGhostY(rowMasks, active.piece, active.rotation, active.x, active.y)
      : null;
  }

  function resetCore(): Mutation {
    clearHistory();
    board.set(initialBoard);
    rebuildRowMasks(board, rowMasks);
    prepareResetRandomState();
    queue.length = 0;
    queueBagIds.length = 0;
    queueHead = 0;
    queueView.length = 0;
    initialQueueIndex = 0;
    currentDrawBagId = null;
    nextRandomBagId = 1;
    active = null;
    activeBagId = null;
    ghostY = null;
    hold = null;
    holdBagId = null;
    holdUsedThisTurn = false;
    canHold = true;
    linesCleared = 0;
    piecesLocked = 0;
    lastClear = 0;
    elapsedMs = 0;
    score = 0;
    combo = 0;
    backToBackChain = 0;
    lastSpin = null;
    lastClearInfo = null;
    status = "running";
    resetPieceTiming();
    ensureQueue();
    const topOut = !spawnPiece(dequeuePiece());
    return { changed: true, locked: false, linesCleared: 0, topOut };
  }

  function startCore(): Mutation {
    if (status === "running") return emptyMutation();
    if (status === "paused") {
      if (!active) return resetCore();
      discardRedoTail();
      status = "running";
      stepAccumulatorMs = 0;
      clearRepeatState();
      return mutationFromChanged(true);
    }
    return resetCore();
  }

  function pauseCore(): Mutation {
    if (status !== "running") return emptyMutation();
    status = "paused";
    stepAccumulatorMs = 0;
    clearRepeatState();
    return mutationFromChanged(true);
  }

  function undoCore(): Mutation {
    if (historyCursor === 0) return emptyMutation();
    historyCursor -= 1;
    restoreHistoryState(history[historyCursor].before, "undo");
    return {
      changed: true,
      locked: false,
      linesCleared: 0,
      topOut: status === "top-out",
    };
  }

  function redoCore(): Mutation {
    if (historyCursor >= history.length) return emptyMutation();
    restoreHistoryState(history[historyCursor].after, "redo");
    historyCursor += 1;
    return {
      changed: true,
      locked: true,
      linesCleared: lastClear,
      topOut: status === "top-out",
    };
  }

  function captureHistoryState(): PlayerHistoryState {
    return {
      board: board.slice(),
      active,
      activeBagId,
      activeWasClutchSpawned,
      status,
      hold,
      holdBagId,
      queue: queue.slice(queueHead),
      queueBagIds: queueBagIds.slice(queueHead),
      currentDrawBagId,
      nextRandomBagId,
      initialQueueIndex,
      randomState,
      holdUsedThisTurn,
      linesCleared,
      piecesLocked,
      lastClear,
      dropAccumulator,
      lockResetCount,
      elapsedMs,
      score,
      pieceSpawnScore,
      combo,
      backToBackChain,
      lastSpin,
      lastClearInfo,
      lastRotation,
    };
  }

  function restoreHistoryState(state: PlayerHistoryState, mode: "undo" | "redo") {
    board.set(state.board);
    rebuildRowMasks(board, rowMasks);
    hold = state.hold;
    holdBagId = state.holdBagId;
    queue.length = 0;
    queue.push(...state.queue);
    queueBagIds.length = 0;
    queueBagIds.push(...state.queueBagIds);
    queueHead = 0;
    currentDrawBagId = state.currentDrawBagId;
    nextRandomBagId = state.nextRandomBagId;
    initialQueueIndex = state.initialQueueIndex;
    randomState = state.randomState;
    holdUsedThisTurn = mode === "undo" ? false : state.holdUsedThisTurn;
    canHold = isUnlimitedHoldEnabled() || !holdUsedThisTurn;
    linesCleared = state.linesCleared;
    piecesLocked = state.piecesLocked;
    lastClear = state.lastClear;
    stepAccumulatorMs = 0;
    dropAccumulator = mode === "undo" ? 0 : state.dropAccumulator;
    lockElapsedMs = 0;
    lockResetCount = 0;
    elapsedMs = state.elapsedMs;
    score = mode === "undo" ? state.pieceSpawnScore : state.score;
    pieceSpawnScore = state.pieceSpawnScore;
    combo = state.combo;
    backToBackChain = state.backToBackChain;
    lastSpin = state.lastSpin;
    lastClearInfo = state.lastClearInfo;
    lastRotation = mode === "undo" ? null : state.lastRotation;
    if (mode === "undo" && state.active) {
      const standard = freezeActive(
        state.active.piece,
        "spawn",
        playerSpawnX(state.active.piece),
        PLAYER_SPAWN_Y,
      );
      active = fits(standard)
        ? standard
        : state.activeWasClutchSpawned
          ? highestHiddenSpawn(state.active.piece)
          : null;
      activeWasClutchSpawned = active !== null && active !== standard;
      activeBagId = active ? state.activeBagId : null;
      status = active ? "running" : "top-out";
    } else {
      active = state.active;
      activeBagId = state.activeBagId;
      activeWasClutchSpawned = state.activeWasClutchSpawned;
      status = state.status === "top-out" ? "top-out" : "running";
    }
    clearRepeatState();
    refreshGhost();
    refreshQueueView();
  }

  function recordLock(before: PlayerHistoryState) {
    discardRedoTail();
    history.push({ before, after: captureHistoryState() });
    if (history.length > PLAYER_HISTORY_LIMIT) history.shift();
    historyCursor = history.length;
  }

  function discardRedoTail() {
    if (historyCursor < history.length) history.length = historyCursor;
  }

  function clearHistory() {
    history.length = 0;
    historyCursor = 0;
  }

  function prepareResetRandomState() {
    if (initialQueue.length > 0) {
      randomState = seed;
      firstRandomBagPending = false;
      return;
    }
    randomState = playerResetSeed(seed, randomSessionIndex);
    randomSessionIndex += 1;
    firstRandomBagPending = true;
  }

  function resetPieceTiming() {
    stepAccumulatorMs = 0;
    dropAccumulator = 0;
    lockElapsedMs = 0;
    lockResetCount = 0;
    lastRotation = null;
    clearRepeatState();
  }

  function isClutchClearEnabled(): boolean {
    return settings.clutchClear;
  }

  function isUnlimitedHoldEnabled(): boolean {
    return settings.unlimitedHold;
  }

  function clearRepeatState() {
    horizontalDirection = 0;
    horizontalHeldMs = 0;
    arrAccumulatorMs = 0;
    instantArrApplied = false;
    previousSoftDrop = false;
  }

  function ensureQueue(requiredMinimum = 0) {
    const minimum = Math.max(settings.previewCount + 7, requiredMinimum);
    while (queue.length - queueHead < minimum) {
      while (initialQueueIndex < initialQueue.length) {
        queue.push(initialQueue[initialQueueIndex]);
        // Arbitrary configured queues intentionally carry no standard-bag
        // boundary claim. Setup search remains unavailable until a generated
        // bag becomes the active source.
        queueBagIds.push(null);
        initialQueueIndex += 1;
      }
      if (queue.length - queueHead >= minimum) break;
      appendBag();
    }
    compactQueue();
  }

  function appendBag() {
    const bag = Array.from(PLAYER_PIECES);
    for (let index = bag.length - 1; index > 0; index -= 1) {
      const swapIndex = Math.floor(nextRandom() * (index + 1));
      [bag[index], bag[swapIndex]] = [bag[swapIndex], bag[index]];
    }
    if (firstRandomBagPending) {
      if (previousFirstRandomBag && playerQueuesEqual(bag, previousFirstRandomBag)) {
        [bag[0], bag[1]] = [bag[1], bag[0]];
      }
      previousFirstRandomBag = Object.freeze(bag.slice());
      firstRandomBagPending = false;
    }
    const bagId = nextRandomBagId;
    nextRandomBagId += 1;
    queue.push(...bag);
    for (let index = 0; index < bag.length; index += 1) queueBagIds.push(bagId);
  }

  function dequeuePiece(): Readonly<{ piece: PlayerPiece; bagId: number | null }> {
    ensureQueue();
    const piece = queue[queueHead];
    const bagId = queueBagIds[queueHead] ?? null;
    currentDrawBagId = bagId;
    queueHead += 1;
    ensureQueue();
    return Object.freeze({ piece, bagId });
  }

  function compactQueue() {
    if (queueHead < 32 || queueHead * 2 < queue.length) return;
    queue.copyWithin(0, queueHead);
    queueBagIds.copyWithin(0, queueHead);
    queue.length -= queueHead;
    queueBagIds.length -= queueHead;
    queueHead = 0;
  }

  function refreshQueueView() {
    queueView.length = 0;
    const count = Math.min(settings.previewCount, queue.length - queueHead);
    for (let index = 0; index < count; index += 1) queueView.push(queue[queueHead + index]);
  }

  function currentSetupBagRemainder(): readonly PlayerPiece[] | null {
    if (!active || activeBagId === null) return null;
    const residue: PlayerPiece[] = [active.piece];
    for (let index = queueHead; index < queue.length; index += 1) {
      if (queueBagIds[index] !== activeBagId) break;
      residue.push(queue[index]);
    }
    return Object.freeze(residue);
  }

  function nextRandom(): number {
    let value = randomState >>> 0;
    value ^= value << 13;
    value ^= value >>> 17;
    value ^= value << 5;
    randomState = value >>> 0 || 0x6d2b79f5;
    return randomState / 0x1_0000_0000;
  }

  function publish() {
    revision += 1;
    syncRenderView();
    for (const listener of listeners) listener(renderView);
  }

  function syncRenderView() {
    renderView.active = active;
    renderView.ghostY = ghostY;
    renderView.hold = hold;
    renderView.status = status;
    renderView.revision = revision;
    renderView.linesCleared = linesCleared;
    renderView.piecesLocked = piecesLocked;
    renderView.lastClear = lastClear;
    renderView.canHold = canHold;
    renderView.lockElapsedMs = lockElapsedMs;
    renderView.lockResetCount = lockResetCount;
    renderView.elapsedMs = elapsedMs;
    renderView.score = score;
    renderView.combo = combo;
    renderView.backToBackChain = backToBackChain;
    renderView.lastSpin = lastSpin;
    renderView.lastClearInfo = lastClearInfo;
    renderView.canUndo = historyCursor > 0;
    renderView.canRedo = historyCursor < history.length;
  }

  function actionResult(mutation: Mutation): PlayerActionResult {
    return Object.freeze({
      changed: mutation.changed,
      locked: mutation.locked,
      linesCleared: mutation.linesCleared,
      topOut: mutation.topOut,
      revision,
    });
  }

  function advanceResult(
    mutation: Mutation,
    steps: number,
    droppedMs: number,
  ): PlayerAdvanceResult {
    return Object.freeze({
      changed: mutation.changed,
      locked: mutation.locked,
      linesCleared: mutation.linesCleared,
      topOut: mutation.topOut,
      steps,
      droppedMs,
      revision,
    });
  }
}

export function playerBoardIndex(x: number, y: number): number {
  if (!Number.isSafeInteger(x) || !Number.isSafeInteger(y)) {
    throw new TypeError("Player board coordinates must be integers.");
  }
  if (x < 0 || x >= PLAYER_BOARD_WIDTH || y < 0 || y >= PLAYER_BOARD_ROWS) {
    throw new RangeError("Player board coordinates are out of range.");
  }
  return y * PLAYER_BOARD_WIDTH + x;
}

function writeBoard(
  target: Uint8Array,
  source: ArrayLike<PlayerBoardInputCell>,
) {
  if (!source || typeof source.length !== "number") {
    throw new TypeError("Player board must be an array-like value.");
  }
  if (!Number.isSafeInteger(source.length) || source.length < 0 || source.length > PLAYER_BOARD_CELLS) {
    throw new RangeError(`Player board must contain at most ${PLAYER_BOARD_CELLS} cells.`);
  }
  if (source.length !== 0 && source.length % PLAYER_BOARD_WIDTH !== 0) {
    throw new RangeError(`Player board length must be a multiple of ${PLAYER_BOARD_WIDTH}.`);
  }
  target.fill(PLAYER_CELL_ID.empty);
  for (let index = 0; index < source.length; index += 1) {
    target[index] = playerCellIdFromCtkColor(source[index]);
  }
}

function rebuildRowMasks(board: Uint8Array, rowMasks: Uint16Array) {
  rowMasks.fill(0);
  for (let row = 0; row < PLAYER_BOARD_ROWS; row += 1) {
    let mask = 0;
    const offset = row * PLAYER_BOARD_WIDTH;
    for (let x = 0; x < PLAYER_BOARD_WIDTH; x += 1) {
      if (board[offset + x] !== PLAYER_CELL_ID.empty) mask |= 1 << x;
    }
    rowMasks[row] = mask;
  }
}

function normalizeInitialQueue(queue: readonly PlayerPiece[] | undefined): readonly PlayerPiece[] {
  if (queue === undefined) return Object.freeze([]);
  if (!Array.isArray(queue)) throw new TypeError("Player initial queue must be an array.");
  const result = Array.from(queue);
  for (const piece of result) {
    if (!(PLAYER_PIECES as readonly string[]).includes(piece)) {
      throw new RangeError(`Unsupported initial queue piece '${String(piece)}'.`);
    }
  }
  return Object.freeze(result);
}

function normalizeSeed(seed: number | string): number {
  if (typeof seed === "number") {
    if (!Number.isFinite(seed)) throw new TypeError("Player seed must be finite.");
    return (Math.trunc(seed) >>> 0) || 0x6d2b79f5;
  }
  if (typeof seed !== "string") throw new TypeError("Player seed must be a number or string.");
  let hash = 0x811c9dc5;
  for (let index = 0; index < seed.length; index += 1) {
    hash ^= seed.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0) || 0x6d2b79f5;
}

function normalizeHeldInput(input: PlayerHeldInput): PlayerHeldInput {
  if (!input || typeof input !== "object") throw new TypeError("Held player input must be an object.");
  const priority = input.horizontalPriority ?? null;
  if (priority !== null && priority !== "left" && priority !== "right") {
    throw new RangeError("horizontalPriority must be left, right, or null.");
  }
  if (
    typeof input.left === "boolean" &&
    typeof input.right === "boolean" &&
    typeof input.softDrop === "boolean" &&
    input.horizontalPriority === priority
  ) {
    return input;
  }
  return {
    left: input.left === true,
    right: input.right === true,
    softDrop: input.softDrop === true,
    horizontalPriority: priority,
  };
}

function playerSettingsEqual(left: PlayerSettings, right: PlayerSettings): boolean {
  return (
    left.gravityG === right.gravityG &&
    left.lockDelayMs === right.lockDelayMs &&
    left.lockResetLimit === right.lockResetLimit &&
    left.dasMs === right.dasMs &&
    left.arrMs === right.arrMs &&
    left.sdf === right.sdf &&
    left.fixedStepMs === right.fixedStepMs &&
    left.maxCatchUpSteps === right.maxCatchUpSteps &&
    left.previewCount === right.previewCount &&
    left.kickProfile === right.kickProfile &&
    left.spinProfile === right.spinProfile &&
    left.scoreProfile === right.scoreProfile &&
    left.clutchClear === right.clutchClear &&
    left.unlimitedHold === right.unlimitedHold &&
    playerScoreModelsEqual(left.scoreModel, right.scoreModel)
  );
}

function isGameplayAction(
  action: PlayerAction["type"] | undefined,
): boolean {
  return (
    action === "move-left" ||
    action === "move-right" ||
    action === "soft-drop" ||
    action === "hard-drop" ||
    action === "rotate-cw" ||
    action === "rotate-ccw" ||
    action === "rotate-180" ||
    action === "hold"
  );
}

function playerScoreModelsEqual(left: PlayerScoreModel, right: PlayerScoreModel): boolean {
  return (
    scoreTablesEqual(left.lineClearScores, right.lineClearScores) &&
    scoreTablesEqual(left.spinScores, right.spinScores) &&
    scoreTablesEqual(left.miniSpinScores, right.miniSpinScores) &&
    scoreTablesEqual(left.perfectClearBonuses, right.perfectClearBonuses) &&
    left.backToBackTetrisPerfectClearBonus === right.backToBackTetrisPerfectClearBonus &&
    left.comboBonusPerStep === right.comboBonusPerStep &&
    left.backToBackMultiplier === right.backToBackMultiplier &&
    left.softDropScorePerCell === right.softDropScorePerCell &&
    left.hardDropScorePerCell === right.hardDropScorePerCell &&
    left.perfectClearMode === right.perfectClearMode
  );
}

function scoreTablesEqual(left: readonly number[], right: readonly number[]): boolean {
  for (let index = 0; index < 5; index += 1) {
    if (left[index] !== right[index]) return false;
  }
  return true;
}

function resolveHorizontalDirection(
  held: PlayerHeldInput,
  previous: -1 | 0 | 1,
): -1 | 0 | 1 {
  if (held.left && !held.right) return -1;
  if (held.right && !held.left) return 1;
  if (!held.left && !held.right) return 0;
  if (held.horizontalPriority === "left") return -1;
  if (held.horizontalPriority === "right") return 1;
  return previous === 0 ? 1 : previous;
}

function freezeActive(
  piece: PlayerPiece,
  rotation: PlayerRotation,
  x: number,
  y: number,
): PlayerActivePiece {
  return Object.freeze({ piece, rotation, x, y });
}

function assertDelta(deltaMs: number) {
  if (typeof deltaMs !== "number" || !Number.isFinite(deltaMs) || deltaMs < 0) {
    throw new RangeError("Player advance delta must be a non-negative finite number.");
  }
}

function emptyMutation(): Mutation {
  return { changed: false, locked: false, linesCleared: 0, topOut: false };
}

function mutationFromChanged(changed: boolean): Mutation {
  return { changed, locked: false, linesCleared: 0, topOut: false };
}

function markHistoryDivergingChange(target: Mutation, changed: boolean) {
  if (!changed) return;
  target.changed = true;
  target.historyDiverging = true;
}

function mergeMutation(target: Mutation, source: Mutation) {
  target.changed = target.changed || source.changed;
  target.locked = target.locked || source.locked;
  target.linesCleared += source.linesCleared;
  target.topOut = target.topOut || source.topOut;
  target.historyDiverging = target.historyDiverging || source.historyDiverging;
}

function actionScoreFor(
  model: PlayerScoreModel,
  cleared: number,
  spin: PlayerSpinInfo | null,
): number {
  const index = Math.min(4, cleared);
  if (spin?.mini) return model.miniSpinScores[index];
  if (spin) return model.spinScores[index];
  return model.lineClearScores[index];
}

function isPlusSpinProfile(profile: PlayerSpinProfile): boolean {
  return (
    profile === "t-spins-plus" ||
    profile === "all-spin-plus" ||
    profile === "all-mini-plus"
  );
}

function boundedScore(value: number): number {
  if (!Number.isFinite(value) || value >= Number.MAX_SAFE_INTEGER) {
    return Number.MAX_SAFE_INTEGER;
  }
  return Math.max(0, Math.floor(value));
}

function playerResetSeed(seed: number, sessionIndex: number): number {
  if (sessionIndex === 0) return seed;
  let value = (seed + Math.imul(sessionIndex >>> 0, 0x9e3779b9)) >>> 0;
  value ^= value >>> 16;
  value = Math.imul(value, 0x85ebca6b) >>> 0;
  value ^= value >>> 13;
  value = Math.imul(value, 0xc2b2ae35) >>> 0;
  value ^= value >>> 16;
  return value >>> 0 || 0x6d2b79f5;
}

function playerQueuesEqual(
  left: readonly PlayerPiece[],
  right: readonly PlayerPiece[],
): boolean {
  if (left.length !== right.length) return false;
  for (let index = 0; index < left.length; index += 1) {
    if (left[index] !== right[index]) return false;
  }
  return true;
}

function tCenter(candidate: PlayerActivePiece): readonly [number, number] {
  if (candidate.rotation === "spawn") return [candidate.x + 1, candidate.y];
  if (candidate.rotation === "right") return [candidate.x, candidate.y + 1];
  return [candidate.x + 1, candidate.y + 1];
}

function tFrontOffsets(
  rotation: PlayerRotation,
): readonly (readonly [number, number])[] {
  if (rotation === "spawn") return T_FRONT_SPAWN;
  if (rotation === "right") return T_FRONT_RIGHT;
  if (rotation === "reverse") return T_FRONT_REVERSE;
  return T_FRONT_LEFT;
}

const BASE_FRAME_MS = 1000 / 60;
const PLAYER_HISTORY_LIMIT = 128;
const PLAYER_LOCK_RESET_HARD_LIMIT = 15;
const T_CORNER_OFFSETS = Object.freeze([
  Object.freeze([-1, -1] as const),
  Object.freeze([1, -1] as const),
  Object.freeze([-1, 1] as const),
  Object.freeze([1, 1] as const),
]);
const T_FRONT_SPAWN = Object.freeze([
  Object.freeze([-1, 1] as const),
  Object.freeze([1, 1] as const),
]);
const T_FRONT_RIGHT = Object.freeze([
  Object.freeze([1, -1] as const),
  Object.freeze([1, 1] as const),
]);
const T_FRONT_REVERSE = Object.freeze([
  Object.freeze([-1, -1] as const),
  Object.freeze([1, -1] as const),
]);
const T_FRONT_LEFT = Object.freeze([
  Object.freeze([-1, -1] as const),
  Object.freeze([-1, 1] as const),
]);
