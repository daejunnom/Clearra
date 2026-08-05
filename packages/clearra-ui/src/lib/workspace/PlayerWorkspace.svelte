<!-- SRP rationale: the single change reason is Player session orchestration; simulation, rendering, controls, settings, and finder execution remain delegated boundaries. -->
<!-- The Player keeps simulation buffers outside Svelte reactivity; this component only bridges input, low-rate HUD state, and the Canvas renderer. -->
<script lang="ts">
  import { Pause, Play, RotateCcw } from '@lucide/svelte';
  import { onDestroy, onMount, tick } from 'svelte';

  import { openFieldDocument } from './fieldInterchange';
  import type { Ctk3Page } from './ctk3Codec';
  import { lockedPageCells, operationCells } from './ctkPageTools';
  import PlayerBoard from './player/PlayerBoard.svelte';
  import PlayerControls from './player/PlayerControls.svelte';
  import PlayerFinderDrawer from './player/PlayerFinderDrawer.svelte';
  import PlayerPiecePreview from './player/PlayerPiecePreview.svelte';
  import PlayerSettingsDrawer from './player/PlayerSettingsDrawer.svelte';
  import { createPlayerGarbageBoard } from './player/playerGarbage';
  import {
    createPlayerEngine,
    type PlayerAction,
    type PlayerEngine,
    type PlayerFinderState,
    type PlayerRenderView,
    type PlayerStatus
  } from './player/playerEngine';
  import { playerFinderBoardIsEmpty } from './player/playerFinderModel';
  import {
    createPlayerInputController,
    shouldIgnorePlayerKeyboardTarget,
    type PlayerControl,
    type PlayerImmediateInputAction,
    type PlayerInputController,
    type PlayerKeyBindingsInput
  } from './player/playerInput';
  import {
    PLAYER_BOARD_ROWS,
    PLAYER_VISIBLE_ROWS,
    type PlayerBoardInputCell,
    type PlayerPiece
  } from './player/playerRules';
  import type { PlayerBoardFrame, PlayerRenderPhase } from './player/playerRenderer';
  import {
    PLAYER_UI_SETTINGS_STORAGE_KEY,
    assignPlayerKeyBinding,
    createDefaultPlayerUiSettings,
    deserializePlayerUiSettings,
    isPlayerHistoryBindingAction,
    isPlayerModifierCode,
    playerKeyboardShortcutFromEvent,
    playerKeyboardShortcutMatches,
    serializePlayerUiSettings,
    type PlayerBindingAction,
    type PlayerUiSettings
  } from './player/playerUiModel';
  import WorkspaceShell from './WorkspaceShell.svelte';
  import {
    preferredWorkspaceLanguage,
    workspaceMessage,
    type WorkspaceLanguage,
    type WorkspaceMessageKey
  } from './workspaceI18n';

  export let workerFactory: (() => Worker) | null = null;
  export let runtime: 'web' | 'desktop' = 'web';

  let language: WorkspaceLanguage = 'en';
  let playerBoard: PlayerBoard;
  let engine: PlayerEngine | null = null;
  let input: PlayerInputController | null = null;
  let uiSettings = createDefaultPlayerUiSettings();
  let status: PlayerStatus = 'idle';
  let holdPiece: PlayerPiece | null = null;
  let nextPieces: PlayerPiece[] = [];
  let linesCleared = 0;
  let piecesLocked = 0;
  let score = 0;
  let combo = 0;
  let backToBackChain = 0;
  let elapsedMs = 0;
  let pps = 0;
  let canUndo = false;
  let canRedo = false;
  let canHold = true;
  let captured = false;
  let settingsOpen = false;
  let finderOpen = false;
  let finderState: PlayerFinderState | null = null;
  let finderSetupVisible = false;
  let capturingAction: PlayerBindingAction | null = null;
  let initialFieldText = '';
  let fieldInvalid = false;
  let notice = '';
  let animationFrame = 0;
  let lastFrameAt = 0;
  let lastHudAt = 0;
  let fieldLoadGeneration = 0;
  let fieldLoadController: AbortController | null = null;
  let baseStartingBoardCells: PlayerBoardInputCell[] = [];
  let destroyed = false;
  let detachInput: (() => void) | null = null;

  const boardFrame: PlayerBoardFrame = {
    cells: new Uint8Array(0),
    boardHeight: PLAYER_BOARD_ROWS,
    visibleRows: PLAYER_VISIBLE_ROWS,
    active: null,
    ghostY: null,
    phase: 'ready'
  };
  const appearance = {
    ghostOpacity: uiSettings.ghostOpacity,
    gridOpacity: uiSettings.gridOpacity
  };

  $: label = (key: WorkspaceMessageKey) => workspaceMessage(language, key);
  $: statusLabel = label(statusMessageKey(status));
  $: primaryActionLabel =
    status === 'running'
      ? label('playerPause')
      : status === 'paused'
        ? label('playerResume')
        : label('playerStart');
  $: elapsedLabel = formatElapsed(elapsedMs);
  $: ppsLabel = Number.isFinite(pps) ? pps.toFixed(2) : '0.00';
  $: scoreLabel = score.toLocaleString(language);

  onMount(async () => {
    language = loadWorkspaceLanguage();
    uiSettings = loadUiSettings();
    syncAppearance();
    engine = createPlayerEngine({
      autoStart: false,
      initialQueue: uiSettings.initialQueue,
      settings: engineSettings(uiSettings)
    });
    input = createPlayerInputController({
      enabled: false,
      bindings: inputBindings(uiSettings),
      onAction: ({ type }) => handleImmediateAction(type)
    });
    detachInput = input.attach(window, document);
    document.addEventListener('visibilitychange', handleVisibilityChange);
    window.addEventListener('blur', handleWindowBlur);
    window.addEventListener('pagehide', handlePageHide);
    await tick();
    renderCurrent(true);
  });

  onDestroy(closePlayer);

  function setLanguage(next: WorkspaceLanguage) {
    language = next;
    try {
      localStorage.setItem('clearra-language', next);
    } catch {
      // Language selection still applies to the current session when storage is unavailable.
    }
  }

  function togglePlaying() {
    if (!engine) return;
    if (status === 'running') {
      pausePlayer();
      return;
    }
    playerBoard?.focus();
    engine.start();
    renderCurrent(true);
    startLoop();
  }

  function restartPlayer() {
    if (!engine) return;
    input?.releaseAll();
    playerBoard?.focus();
    engine.reset();
    elapsedMs = 0;
    renderCurrent(true);
    startLoop();
  }

  function pausePlayer() {
    if (!engine) return;
    engine.pause();
    input?.releaseAll();
    stopLoop();
    renderCurrent(true);
  }

  function handleImmediateAction(type: PlayerImmediateInputAction) {
    if (!engine) return;
    if (type === 'reset') {
      restartPlayer();
      return;
    }
    if (type === 'toggle-pause') {
      if (status === 'running') pausePlayer();
      else {
        input?.releaseAll();
        togglePlaying();
      }
      return;
    }
    if (status === 'paused') return;
    if (status !== 'running') engine.start();
    const result = engine.dispatch(type as PlayerAction['type']);
    if (result.locked) applySpawnBuffers();
    renderCurrent(true);
    startLoop();
  }

  function handleBoardCapture(event: CustomEvent<boolean>) {
    captured = event.detail;
    input?.setEnabled(captured && capturingAction === null);
    if (!captured && status === 'running') pausePlayer();
  }

  function handleBoardActivate() {
    if (status !== 'running') togglePlaying();
  }

  function handleGlobalKeyDown(event: KeyboardEvent) {
    if (capturingAction) {
      if (event.repeat) return;
      if (event.code === 'Tab' || shouldIgnorePlayerKeyboardTarget(event.target)) {
        cancelBindingCapture();
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      event.stopImmediatePropagation();
      const historyBinding = isPlayerHistoryBindingAction(capturingAction);
      if (historyBinding && isPlayerModifierCode(event.code)) return;
      const assignment = assignPlayerKeyBinding(
        uiSettings,
        capturingAction,
        historyBinding ? playerKeyboardShortcutFromEvent(event) : event.code
      );
      if (assignment.ok) {
        applyUiSettings(assignment.settings);
        capturingAction = null;
        notice = '';
      } else {
        notice = label('playerBindingDuplicate');
      }
      input?.setEnabled(captured && capturingAction === null);
      return;
    }
    if (finderOpen) {
      if (event.code === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        event.stopImmediatePropagation();
        setFinderOpen(false);
      }
      return;
    }
    const historyAction = playerHistoryShortcut(event);
    if (historyAction && !shouldIgnorePlayerKeyboardTarget(event.target)) {
      event.preventDefault();
      event.stopPropagation();
      event.stopImmediatePropagation();
      if (event.repeat) return;
      if (historyAction === 'undo') undoPlayer();
      else redoPlayer();
      return;
    }
    if (settingsOpen && event.code === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      setSettingsOpen(false);
      return;
    }
    if (shouldStartPlayerFromGlobalEnter(event)) {
      event.preventDefault();
      event.stopPropagation();
      event.stopImmediatePropagation();
      playerBoard?.focus();
      if (engine?.status !== 'running') {
        engine?.start();
        renderCurrent(true);
        startLoop();
      }
    }
  }

  function playerHistoryShortcut(event: KeyboardEvent): 'undo' | 'redo' | null {
    if (playerKeyboardShortcutMatches(event, uiSettings.bindings.undo)) return 'undo';
    if (playerKeyboardShortcutMatches(event, uiSettings.bindings.redo)) return 'redo';
    // Keep the conventional macOS aliases while the Windows defaults are selected.
    if (
      uiSettings.bindings.undo === 'Control+KeyZ' &&
      playerKeyboardShortcutMatches(event, 'Meta+KeyZ')
    ) return 'undo';
    if (
      uiSettings.bindings.redo === 'Control+KeyY' &&
      (playerKeyboardShortcutMatches(event, 'Meta+KeyY') ||
        playerKeyboardShortcutMatches(event, 'Control+Shift+KeyZ') ||
        playerKeyboardShortcutMatches(event, 'Meta+Shift+KeyZ'))
    ) return 'redo';
    return null;
  }

  function undoPlayer() {
    if (!engine || !engine.undo().changed) return;
    input?.releaseAll();
    renderCurrent(true);
    if (engine.status === 'running') startLoop();
    else stopLoop();
  }

  function redoPlayer() {
    if (!engine || !engine.redo().changed) return;
    input?.releaseAll();
    renderCurrent(true);
    if (engine.status === 'running') startLoop();
    else stopLoop();
  }

  function shouldStartPlayerFromGlobalEnter(event: KeyboardEvent): boolean {
    if (
      settingsOpen ||
      finderOpen ||
      event.repeat ||
      event.ctrlKey ||
      event.metaKey ||
      event.altKey ||
      (event.code !== 'Enter' && event.code !== 'NumpadEnter') ||
      shouldIgnorePlayerKeyboardTarget(event.target)
    ) return false;
    const target = event.target;
    if (!(target instanceof Element)) return target === document || target === window;
    return (
      target === document.body ||
      target === document.documentElement ||
      target.closest('[data-player-board-surface]') !== null
    );
  }

  function requestBindingCapture(event: CustomEvent<{ action: PlayerBindingAction }>) {
    capturingAction = event.detail.action;
    notice = '';
    input?.setEnabled(false);
  }

  function cancelBindingCapture() {
    capturingAction = null;
    notice = '';
    input?.setEnabled(captured);
  }

  function setSettingsOpen(next: boolean) {
    if (settingsOpen === next) return;
    if (next && finderOpen) {
      finderOpen = false;
      finderState = null;
    }
    settingsOpen = next;
    if (next) pausePlayer();
    else if (capturingAction) cancelBindingCapture();
  }

  function setFinderOpen(next: boolean) {
    if (finderOpen === next) return;
    if (next) {
      if (settingsOpen) setSettingsOpen(false);
      input?.releaseAll();
      if (engine?.status === 'idle') engine.start();
      if (engine?.status === 'running') engine.pause();
      stopLoop();
      finderState = engine?.getFinderState() ?? null;
      finderSetupVisible = finderState
        ? playerFinderBoardIsEmpty(finderState.rowMasks)
        : false;
      renderCurrent(true);
    } else {
      finderState = null;
      finderSetupVisible = false;
    }
    finderOpen = next;
  }

  function applyUiSettings(next: PlayerUiSettings) {
    uiSettings = next;
    syncAppearance();
    engine?.updateSettings(engineSettings(next));
    input?.setBindings(inputBindings(next));
    try {
      localStorage.setItem(
        PLAYER_UI_SETTINGS_STORAGE_KEY,
        serializePlayerUiSettings(next)
      );
    } catch {
      // A storage failure must not interrupt the local game session.
    }
    renderCurrent(true);
  }

  async function loadStartingField(source: string) {
    const generation = ++fieldLoadGeneration;
    fieldLoadController?.abort();
    const controller = new AbortController();
    fieldLoadController = controller;
    fieldInvalid = false;
    let documentReader: ReturnType<typeof openFieldDocument> | null = null;
    try {
      documentReader = openFieldDocument(source, {
        cacheSegments: 1,
        workers: 1,
        workerFactory: null,
        signal: controller.signal
      });
      if (documentReader.width !== 10 || documentReader.pageCount < 1) {
        throw new RangeError('Player fields must have ten columns and at least one page.');
      }
      const page = await documentReader.readPage(0);
      if (generation !== fieldLoadGeneration || destroyed) return;
      const cells = playerStartingBoardCells(page);
      baseStartingBoardCells = cells.slice();
      engine?.loadBoard(cells);
      engine?.pause();
      elapsedMs = 0;
      notice = label('playerFieldLoaded');
      renderCurrent(true);
      stopLoop();
    } catch {
      if (controller.signal.aborted || generation !== fieldLoadGeneration || destroyed) return;
      fieldInvalid = true;
      notice = '';
    } finally {
      documentReader?.close();
      if (fieldLoadController === controller) fieldLoadController = null;
    }
  }

  function clearStartingField() {
    fieldLoadGeneration += 1;
    fieldLoadController?.abort();
    fieldLoadController = null;
    engine?.loadBoard([]);
    baseStartingBoardCells = [];
    engine?.pause();
    elapsedMs = 0;
    fieldInvalid = false;
    notice = label('playerFieldCleared');
    renderCurrent(true);
    stopLoop();
  }

  function applyGarbage(lines: number, holeSpreadPercent: number) {
    if (!engine) return;
    fieldLoadGeneration += 1;
    fieldLoadController?.abort();
    fieldLoadController = null;
    const result = createPlayerGarbageBoard({
      lines,
      holeSpreadPercent,
      initialBoard: baseStartingBoardCells
    });
    // The generator writes only validated PlayerCellId values into this typed buffer.
    engine.loadBoard(result.board as unknown as ArrayLike<PlayerBoardInputCell>);
    engine.pause();
    elapsedMs = 0;
    notice = label(
      result.overflowed
        ? 'playerGarbageOverflow'
        : lines === 0
          ? 'playerGarbageCleared'
          : 'playerGarbageApplied'
    );
    renderCurrent(true);
    stopLoop();
  }

  function applyInitialQueue(queue: readonly PlayerPiece[]) {
    if (!engine) return;
    input?.releaseAll();
    engine.loadQueue(queue);
    engine.pause();
    elapsedMs = 0;
    notice = label(queue.length > 0 ? 'playerInitialQueueApplied' : 'playerRandomBagApplied');
    renderCurrent(true);
    stopLoop();
  }

  function startLoop() {
    if (!engine || engine.status !== 'running' || animationFrame) return;
    lastFrameAt = performance.now();
    animationFrame = requestAnimationFrame(runFrame);
  }

  function runFrame(now: number) {
    animationFrame = 0;
    if (destroyed || !engine || engine.status !== 'running') return;
    const deltaMs = Math.max(0, now - lastFrameAt);
    lastFrameAt = now;
    const result = engine.advance(deltaMs, input?.held);
    if (result.locked) applySpawnBuffers();
    if (result.changed) renderCurrent(false);
    if (now - lastHudAt >= 100) updateHud(engine.getRenderView(), now);
    if (engine.status === 'running') {
      animationFrame = requestAnimationFrame(runFrame);
    } else {
      renderCurrent(true);
    }
  }

  function applySpawnBuffers() {
    if (!engine || engine.status !== 'running' || !input) return;
    const controller = input;
    const pressed = (control: PlayerControl) => controller.isPressed(control);
    if (uiSettings.ihs && pressed('hold')) engine.dispatch('hold');
    if (!uiSettings.irs) return;
    if (pressed('rotate180')) engine.dispatch('rotate-180');
    else if (pressed('rotateCw')) engine.dispatch('rotate-cw');
    else if (pressed('rotateCcw')) engine.dispatch('rotate-ccw');
  }

  function renderCurrent(forceHud: boolean) {
    if (!engine || !playerBoard) return;
    const view = engine.getRenderView();
    boardFrame.cells = view.board;
    boardFrame.active = view.active;
    boardFrame.ghostY = view.ghostY;
    boardFrame.phase = renderPhase(view.status);
    playerBoard.renderFrame(boardFrame, appearance);
    if (forceHud || performance.now() - lastHudAt >= 100) {
      updateHud(view, performance.now());
    }
  }

  function updateHud(view: PlayerRenderView, now: number) {
    const queueChanged =
      view.queue.length !== nextPieces.length ||
      view.queue.some((piece, index) => piece !== nextPieces[index]);
    status = view.status;
    holdPiece = view.hold;
    if (queueChanged) nextPieces = Array.from(view.queue);
    linesCleared = view.linesCleared;
    piecesLocked = view.piecesLocked;
    score = view.score;
    combo = view.combo;
    backToBackChain = view.backToBackChain;
    elapsedMs = view.elapsedMs;
    pps = elapsedMs > 0 ? piecesLocked / (elapsedMs / 1000) : 0;
    canUndo = view.canUndo;
    canRedo = view.canRedo;
    canHold = view.canHold;
    lastHudAt = now;
  }

  function syncAppearance() {
    appearance.ghostOpacity = uiSettings.ghostOpacity;
    appearance.gridOpacity = uiSettings.gridOpacity;
  }

  function loadUiSettings(): PlayerUiSettings {
    try {
      const source = localStorage.getItem(PLAYER_UI_SETTINGS_STORAGE_KEY);
      return source ? deserializePlayerUiSettings(source) : createDefaultPlayerUiSettings();
    } catch {
      try {
        localStorage.removeItem(PLAYER_UI_SETTINGS_STORAGE_KEY);
      } catch {
        // Read-only or unavailable storage still permits an in-memory session.
      }
      return createDefaultPlayerUiSettings();
    }
  }

  function loadWorkspaceLanguage(): WorkspaceLanguage {
    try {
      return preferredWorkspaceLanguage(
        localStorage.getItem('clearra-language') ?? navigator.language
      );
    } catch {
      return preferredWorkspaceLanguage(navigator.language);
    }
  }

  function playerStartingBoardCells(page: Ctk3Page) {
    let requiredHeight = page.height;
    if (page.operation && page.flags?.lock !== false) {
      for (const cell of operationCells(page.operation)) {
        requiredHeight = Math.max(requiredHeight, cell.y + 1);
      }
    }
    if (requiredHeight > PLAYER_BOARD_ROWS) {
      throw new RangeError('Player field exceeds the hidden board buffer.');
    }
    return lockedPageCells(
      requiredHeight === page.height ? page : { ...page, height: requiredHeight }
    );
  }

  function handleVisibilityChange() {
    if (document.hidden) pausePlayer();
  }

  function handleWindowBlur() {
    pausePlayer();
  }

  function handlePageHide() {
    pausePlayer();
  }

  function stopLoop() {
    if (!animationFrame) return;
    cancelAnimationFrame(animationFrame);
    animationFrame = 0;
  }

  function closePlayer() {
    if (destroyed) return;
    destroyed = true;
    fieldLoadGeneration += 1;
    fieldLoadController?.abort();
    fieldLoadController = null;
    stopLoop();
    detachInput?.();
    input?.dispose();
    if (typeof document !== 'undefined') {
      document.removeEventListener('visibilitychange', handleVisibilityChange);
    }
    if (typeof window !== 'undefined') {
      window.removeEventListener('blur', handleWindowBlur);
      window.removeEventListener('pagehide', handlePageHide);
    }
  }

  function engineSettings(settings: PlayerUiSettings) {
    return {
      gravityG: settings.gravityG,
      lockDelayMs: settings.lockDelayMs,
      lockResetLimit: settings.lockResetLimit,
      dasMs: settings.dasMs,
      arrMs: settings.arrMs,
      sdf: settings.softDropFactor,
      kickProfile: settings.kickProfile,
      spinProfile: settings.spinProfile,
      scoreProfile: settings.scoreProfile,
      scoreModel: settings.scoreModel,
      clutchClear: settings.clutchClear,
      unlimitedHold: settings.unlimitedHold
    };
  }

  function inputBindings(settings: PlayerUiSettings): PlayerKeyBindingsInput {
    const bindings = settings.bindings;
    return {
      moveLeft: bindings['move-left'],
      moveRight: bindings['move-right'],
      softDrop: bindings['soft-drop'],
      hardDrop: bindings['hard-drop'],
      rotateCcw: bindings['rotate-ccw'],
      rotateCw: bindings['rotate-cw'],
      rotate180: bindings['rotate-180'],
      hold: bindings.hold,
      reset: bindings.restart,
      togglePause: bindings.pause
    };
  }

  function ariaKeyboardShortcut(shortcut: string): string {
    return shortcut
      .split('+')
      .map((part) => {
        if (part.startsWith('Key') && part.length === 4) return part.slice(3);
        if (part.startsWith('Digit') && part.length === 6) return part.slice(5);
        return part;
      })
      .join('+');
  }

  function statusMessageKey(value: PlayerStatus): WorkspaceMessageKey {
    if (value === 'running') return 'playerRunning';
    if (value === 'paused') return 'playerPaused';
    if (value === 'top-out') return 'playerGameOver';
    return 'playerReady';
  }

  function renderPhase(value: PlayerStatus): PlayerRenderPhase {
    if (value === 'running') return 'playing';
    if (value === 'paused') return 'paused';
    if (value === 'top-out') return 'game-over';
    return 'ready';
  }

  function formatElapsed(milliseconds: number): string {
    const totalSeconds = Math.max(0, milliseconds) / 1000;
    const minutes = Math.floor(totalSeconds / 60);
    const seconds = totalSeconds - minutes * 60;
    return `${minutes}:${seconds.toFixed(2).padStart(5, '0')}`;
  }
</script>

<svelte:window on:keydown|capture={handleGlobalKeyDown} />

<svelte:head>
  <title>{label('player')} · Clearra</title>
  <meta name="description" content={label('playerPageDescription')} />
  <meta property="og:title" content={`${label('player')} · Clearra`} />
  <meta property="og:description" content={label('playerPageDescription')} />
</svelte:head>

<WorkspaceShell
  activeMode="player"
  {language}
  active={false}
  statusActive={status === 'running'}
  {statusLabel}
  workspaceLabel={label('workspaceMode')}
  dimensionLabel=""
  dimensionValue={PLAYER_VISIBLE_ROWS}
  cancelLabel=""
  runLabel=""
  showDimension={false}
  showActions={false}
  editorOnly
  on:language={(event) => setLanguage(event.detail)}
>
  <svelte:fragment slot="editor">
    <section class="player-stage" aria-label={label('playerBoard')}>
      <PlayerSettingsDrawer
        title={label('playerSettings')}
        openLabel={label('playerOpenSettings')}
        closeLabel={label('playerCloseSettings')}
        open={settingsOpen}
        on:openchange={(event) => setSettingsOpen(event.detail)}
      >
        <PlayerControls
          {language}
          settings={uiSettings}
          {capturingAction}
          {initialFieldText}
          {fieldInvalid}
          on:settingschange={(event) => applyUiSettings(event.detail)}
          on:capturebinding={requestBindingCapture}
          on:cancelbinding={cancelBindingCapture}
          on:restoredefaults={() => (notice = label('playerSettingsReset'))}
          on:fieldinput={(event) => {
            initialFieldText = event.detail.source;
            fieldInvalid = false;
          }}
          on:loadfield={(event) => void loadStartingField(event.detail.source)}
          on:clearfield={clearStartingField}
          on:applyqueue={(event) => applyInitialQueue(event.detail.queue)}
          on:applygarbage={(event) =>
            applyGarbage(event.detail.lines, event.detail.holeSpreadPercent)}
        />
      </PlayerSettingsDrawer>

      <PlayerFinderDrawer
        {language}
        {runtime}
        {workerFactory}
        open={finderOpen}
        state={finderState}
        setupVisible={finderSetupVisible}
        on:openchange={(event) => setFinderOpen(event.detail)}
      />

      {#if notice}<p class="notice" role="status">{notice}</p>{/if}

      <div class="playfield-layout">
        <div class="board-column">
          <PlayerBoard
            bind:this={playerBoard}
            boardLabel={label('playerBoard')}
            focusHint={label('playerFocusHint')}
            activeHint={label('playerControlsActive')}
            readyLabel={label('playerReady')}
            pausedLabel={label('playerPaused')}
            gameOverLabel={label('playerGameOver')}
            on:capture={handleBoardCapture}
            on:activate={handleBoardActivate}
          />
        </div>

        <aside class="left-rail" aria-label={label('playerInfo')}>
          <div class="piece-card hold-card">
            <span>{label('playerHold')}</span>
            <PlayerPiecePreview
              piece={holdPiece}
              muted={holdPiece !== null && !canHold}
              label={label(holdPiece !== null && !canHold ? 'playerHoldUnavailable' : 'playerHold')}
            />
          </div>
          <dl class="stats-card">
            <div><dt>{label('playerScore')}</dt><dd>{scoreLabel}</dd></div>
            <div><dt>{label('playerLines')}</dt><dd>{linesCleared}</dd></div>
            <div><dt>{label('playerCombo')}</dt><dd>{combo}</dd></div>
            <div><dt>{label('playerB2b')}</dt><dd>{backToBackChain}</dd></div>
            <div><dt>{label('playerTime')}</dt><dd>{elapsedLabel}</dd></div>
            <div><dt>{label('playerPps')}</dt><dd>{ppsLabel}</dd></div>
            <div><dt>{label('playerPieces')}</dt><dd>{piecesLocked}</dd></div>
          </dl>
          <div class="game-actions">
            <button class="primary" type="button" on:pointerdown|preventDefault on:click={togglePlaying}>
              {#if status === 'running'}<Pause size={15} fill="currentColor" />{:else}<Play size={15} fill="currentColor" />{/if}
              {primaryActionLabel}
            </button>
            <button type="button" on:pointerdown|preventDefault on:click={restartPlayer}>
              <RotateCcw size={15} />{label('playerRestart')}
            </button>
            <div class="history-actions">
              <button
                type="button"
                disabled={!canUndo}
                aria-keyshortcuts={ariaKeyboardShortcut(uiSettings.bindings.undo)}
                on:pointerdown|preventDefault
                on:click={undoPlayer}
              >
                {label('playerUndo')}
              </button>
              <button
                type="button"
                disabled={!canRedo}
                aria-keyshortcuts={ariaKeyboardShortcut(uiSettings.bindings.redo)}
                on:pointerdown|preventDefault
                on:click={redoPlayer}
              >
                {label('playerRedo')}
              </button>
            </div>
          </div>
        </aside>

        <aside class="right-rail" aria-label={label('playerNext')}>
          <div class="piece-card next-card">
            <span>{label('playerNext')}</span>
            <ol class="next-list">
              {#each nextPieces as piece, index (`${index}:${piece}`)}
                <li>
                  <PlayerPiecePreview {piece} compact label={`${label('playerNext')} ${index + 1}: ${piece}`} />
                </li>
              {/each}
            </ol>
          </div>
        </aside>
      </div>
    </section>
  </svelte:fragment>
</WorkspaceShell>

<style>
  .player-stage {
    margin: 0 auto;
    max-width: 760px;
    min-width: 0;
  }

  .playfield-layout {
    align-items: start;
    display: grid;
    gap: 8px;
    grid-template-areas: 'left board next';
    grid-template-columns: 116px minmax(260px, 440px) 104px;
    justify-content: center;
  }

  .left-rail,
  .right-rail {
    display: grid;
    gap: 8px;
    min-width: 0;
  }

  .left-rail { grid-area: left; }
  .board-column { grid-area: board; min-width: 0; }
  .right-rail { grid-area: next; }

  .game-actions {
    display: grid;
    gap: 7px;
  }

  .game-actions button {
    align-items: center;
    background: #fff;
    border: 1px solid #bcc8c2;
    border-radius: 5px;
    color: #3f4d47;
    cursor: pointer;
    display: inline-flex;
    font-size: 11px;
    font-weight: 750;
    gap: 7px;
    justify-content: center;
    min-height: 36px;
    padding: 0 8px;
    width: 100%;
  }

  .game-actions button.primary {
    background: #0d7168;
    border-color: #0d7168;
    color: #fff;
  }

  .history-actions {
    display: grid;
    gap: 7px;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .history-actions button {
    font-size: 10px;
    padding: 0 4px;
  }

  .game-actions button:disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }

  .piece-card,
  .stats-card {
    background: #f6f8f7;
    border: 1px solid #d7dfda;
    border-radius: 6px;
    margin: 0;
    min-width: 0;
    padding: 9px;
  }

  .piece-card > span {
    color: #65716c;
    display: block;
    font-size: 9px;
    font-weight: 800;
    margin-bottom: 7px;
    text-align: center;
    text-transform: uppercase;
  }

  .next-list {
    align-items: center;
    display: grid;
    gap: 8px;
    justify-content: center;
    list-style: none;
    margin: 0;
    min-height: 24px;
    padding: 0;
  }

  .next-list li { min-width: 0; }

  .stats-card {
    display: grid;
    gap: 9px;
    grid-template-columns: minmax(0, 1fr);
  }

  .stats-card div {
    min-width: 0;
  }

  dt {
    color: #6a7671;
    font-size: 8px;
    font-weight: 800;
    text-transform: uppercase;
  }

  dd {
    color: #17211e;
    font: 750 14px ui-monospace, SFMono-Regular, Consolas, monospace;
    margin: 2px 0 0;
  }

  .notice {
    background: #eaf4f1;
    border: 1px solid #b9d5ce;
    border-radius: 6px;
    color: #075f58;
    font-size: 10px;
    line-height: 1.4;
    margin: 0 0 10px;
    padding: 9px 11px;
  }

  @media (max-width: 720px) {
    .playfield-layout {
      grid-template-areas:
        'board board'
        'left next';
      grid-template-columns: minmax(0, 1fr) minmax(86px, 104px);
      margin: 0 auto;
      max-width: 548px;
    }

    .left-rail {
      grid-template-columns: minmax(92px, .7fr) minmax(170px, 1.4fr) minmax(110px, .9fr);
    }

    .stats-card {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }

  @media (max-width: 620px) {
    .playfield-layout {
      grid-template-areas:
        'board'
        'left'
        'next';
      grid-template-columns: minmax(0, 1fr);
    }

    .left-rail {
      grid-template-columns: minmax(0, 1fr);
    }

    .stats-card {
      grid-column: auto;
      grid-row: auto;
    }
  }
</style>
