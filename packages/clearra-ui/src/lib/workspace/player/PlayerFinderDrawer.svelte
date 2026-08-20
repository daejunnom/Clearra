<script lang="ts">
  import { Search, Square, X } from '@lucide/svelte';
  import { createEventDispatcher, getContext, onDestroy, onMount, tick } from 'svelte';
  import { get } from 'svelte/store';

  import {
    cancelDesktopJob,
    clearDesktopTerminalResult,
    desktopJobState,
    disposeDesktopJobPolling,
    resumeDesktopJobPolling,
    startDesktopJob,
    updateDesktopRequest
  } from '../../stores';
  import {
    CPU_ONLY_RUNTIME_WARMUP_POLICY,
    DEFAULT_RUNTIME_WARMUP_POLICY,
    HOST_CAPABILITY_SNAPSHOT_CONTEXT,
    automaticWorkerAuthority,
    clearWasmTerminalResult,
    sharedBrowserHostCapabilitySnapshot,
    updateWasmCommandText,
    wasmWorkerState,
    WasmTerminalWorkerController,
    type HostCapabilitySnapshot
  } from '../../wasm';
  import PcSolverResult from '../PcSolverResult.svelte';
  import SetupFinderResult from '../SetupFinderResult.svelte';
  import { setupFinderRequestForDesktop } from '../setupFinderModel';
  import {
    buildWorkspaceCommand,
    workspaceRequestForDesktop
  } from '../solverWorkspaceModel';
  import {
    workspaceMessage,
    type WorkspaceLanguage,
    type WorkspaceMessageKey
  } from '../workspaceI18n';
  import {
    workspaceViewFromDesktop,
    workspaceViewFromWasm,
    type WorkspaceRuntimeStatus
  } from '../workspaceRuntime';
  import type { PlayerFinderState } from './playerEngine';
  import {
    buildPlayerSetupFinderCommand,
    preparePlayerPcFinder,
    preparePlayerSetupFinder,
    type PlayerFinderIssue,
    type PlayerPcQueueMode
  } from './playerFinderModel';

  export let language: WorkspaceLanguage;
  export let runtime: 'web' | 'desktop' = 'web';
  export let workerFactory: (() => Worker) | null = null;
  export let open = false;
  export let state: PlayerFinderState | null = null;
  export let setupVisible = false;

  type FinderKind = 'pc' | 'setup';

  const dispatch = createEventDispatcher<{ openchange: boolean }>();
  const hostCapabilitySnapshot =
    getContext<HostCapabilitySnapshot>(HOST_CAPABILITY_SNAPSHOT_CONTEXT) ??
    sharedBrowserHostCapabilitySnapshot();
  const workerController = new WasmTerminalWorkerController(
    workerFactory,
    hostCapabilitySnapshot
  );
  let launcher: HTMLButtonElement;
  let closeButton: HTMLButtonElement;
  let drawer: HTMLElement;
  let selected: FinderKind = 'pc';
  let pcQueueMode: PlayerPcQueueMode = 'queue-based';
  let visibleRangeOnly = false;
  let resultKind: FinderKind | null = null;
  let resultTargetLines = 4;
  let setupSearchMode: 'oracle' | 'qb' = 'oracle';
  let issue: PlayerFinderIssue | null = null;
  let elapsedMs = 0;
  let runStartedAt = 0;
  let elapsedTimer: ReturnType<typeof setInterval> | null = null;
  const hardwareConcurrency = hostCapabilitySnapshot.reportedLogicalProcessors;
  let lastStateRevision: number | null = null;
  let previousOpen = open;
  let restoreLauncherFocus = true;
  let disposed = false;

  $: workerController.setWorkerFactory(workerFactory);
  $: runtimeView = runtime === 'web'
    ? workspaceViewFromWasm($wasmWorkerState)
    : workspaceViewFromDesktop($desktopJobState);
  $: active = runtimeView.status === 'running' || runtimeView.status === 'cancelling';
  $: pcPreparation = state
    ? preparePlayerPcFinder(state, {
        hardwareConcurrency,
        queueMode: pcQueueMode,
        visibleRangeOnly
      })
    : null;
  $: setupPreparation = state ? preparePlayerSetupFinder(state) : null;
  $: selectedPreparation = selected === 'pc' ? pcPreparation : setupPreparation;
  $: selectedIssue = issue ?? (selectedPreparation && !selectedPreparation.ok
    ? selectedPreparation.issue
    : null);
  $: if ((state?.revision ?? null) !== lastStateRevision) {
    lastStateRevision = state?.revision ?? null;
    issue = null;
    resultKind = null;
  }
  $: label = (
    key: WorkspaceMessageKey,
    values: Record<string, string | number> = {}
  ) => workspaceMessage(language, key, values);
  $: if (!setupVisible && selected === 'setup') selected = 'pc';
  $: if (isTerminal(runtimeView.status) && elapsedTimer !== null) stopElapsedTimer();
  $: if (open !== previousOpen) {
    const shouldRestoreLauncher = restoreLauncherFocus;
    previousOpen = open;
    if (!open && active) void cancel();
    void tick().then(() => {
      if (open) closeButton?.focus({ preventScroll: true });
      else if (shouldRestoreLauncher) launcher?.focus({ preventScroll: true });
      restoreLauncherFocus = true;
    });
  }

  onMount(() => {
    if (runtime === 'desktop') resumeDesktopJobPolling();
  });

  onDestroy(dispose);

  function requestOpen(next: boolean, restoreFocus = true) {
    restoreLauncherFocus = restoreFocus;
    if (open !== next) dispatch('openchange', next);
  }

  function selectFinder(next: FinderKind) {
    if (active || (next === 'setup' && !setupVisible)) return;
    if (selected !== next) {
      resultKind = null;
      elapsedMs = 0;
      runStartedAt = 0;
    }
    selected = next;
    issue = null;
  }

  function changePcQueueMode() {
    if (active) return;
    invalidatePcResult();
  }

  function invalidatePcResult() {
    issue = null;
    resultKind = null;
    elapsedMs = 0;
    runStartedAt = 0;
    if (runtime === 'web') clearWasmTerminalResult();
    else clearDesktopTerminalResult();
  }

  async function runSelected() {
    if (!state || active) return;
    issue = null;
    if (selected === 'pc') {
      const prepared = preparePlayerPcFinder(state, {
        hardwareConcurrency,
        queueMode: pcQueueMode,
        visibleRangeOnly
      });
      if (!prepared.ok) {
        issue = prepared.issue;
        return;
      }
      resultKind = 'pc';
      resultTargetLines = prepared.targetLines;
      startElapsedTimer();
      if (runtime === 'web') {
        clearWasmTerminalResult();
        workerController.prewarm(
          prepared.request.workers,
          false,
          DEFAULT_RUNTIME_WARMUP_POLICY,
          automaticWorkerAuthority(hostCapabilitySnapshot)
        );
        updateWasmCommandText(buildWorkspaceCommand(prepared.request));
        if (!workerController.run()) stopElapsedTimer();
      } else {
        clearDesktopTerminalResult();
        updateDesktopRequest(workspaceRequestForDesktop(prepared.request, language));
        await startDesktopJob();
      }
      return;
    }

    const prepared = preparePlayerSetupFinder(state);
    if (!prepared.ok) {
      issue = prepared.issue;
      return;
    }
    const workers = automaticWorkerAuthority(hostCapabilitySnapshot).workersEffective;
    resultKind = 'setup';
    setupSearchMode = prepared.request.searchMode;
    startElapsedTimer();
    if (runtime === 'web') {
      clearWasmTerminalResult();
      workerController.prewarm(
        workers,
        false,
        CPU_ONLY_RUNTIME_WARMUP_POLICY,
        automaticWorkerAuthority(hostCapabilitySnapshot)
      );
      updateWasmCommandText(buildPlayerSetupFinderCommand(prepared.request, workers));
      if (!workerController.run()) stopElapsedTimer();
    } else {
      clearDesktopTerminalResult();
      updateDesktopRequest(
        setupFinderRequestForDesktop(prepared.request, language, workers)
      );
      await startDesktopJob();
    }
  }

  async function cancel() {
    if (!active) return;
    if (runtime === 'web') workerController.cancel();
    else await cancelDesktopJob();
  }

  function handleWindowPointerDown(event: PointerEvent) {
    if (!open) return;
    const path = event.composedPath();
    if (path.includes(drawer) || path.includes(launcher)) return;
    requestOpen(false, false);
  }

  function startElapsedTimer() {
    stopElapsedTimer();
    elapsedMs = 0;
    runStartedAt = performance.now();
    elapsedTimer = setInterval(() => {
      elapsedMs = performance.now() - runStartedAt;
    }, 100);
  }

  function stopElapsedTimer() {
    if (elapsedTimer !== null) clearInterval(elapsedTimer);
    elapsedTimer = null;
    if (runStartedAt > 0) elapsedMs = performance.now() - runStartedAt;
  }

  function dispose() {
    if (disposed) return;
    disposed = true;
    stopElapsedTimer();
    if (runtime === 'web') {
      workerController.dispose();
      clearWasmTerminalResult();
    } else {
      const desktopState = get(desktopJobState);
      const desktopJobActive =
        desktopState.jobId !== null ||
        desktopState.status === 'running' ||
        desktopState.status === 'cancelling';
      if (desktopJobActive) {
        // The shared poller must observe the terminal cancellation event or a
        // route change can strand the native job in a permanent busy state.
        void cancelDesktopJob();
      } else {
        disposeDesktopJobPolling();
        clearDesktopTerminalResult();
      }
    }
  }

  function issueMessageKey(value: PlayerFinderIssue): WorkspaceMessageKey {
    switch (value) {
      case 'no-active-piece': return 'playerFinderNoActivePiece';
      case 'hold-already-used': return 'playerFinderHoldAlreadyUsed';
      case 'unlimited-hold-unsupported': return 'playerFinderUnlimitedHoldUnsupported';
      case 'board-above-pc-limit': return 'playerFinderBoardAboveLimit';
      case 'no-feasible-pc-target': return 'playerFinderNoPcTarget';
      case 'pc-bag-boundary-unknown': return 'playerFinderPcBagUnknown';
      case 'pc-residue-invalid': return 'playerFinderPcResidueInvalid';
      case 'pc-pattern-universe-too-large': return 'playerFinderPcUniverseTooLarge';
      case 'setup-board-not-empty': return 'playerFinderSetupBoardNotEmpty';
      case 'setup-hold-unsupported': return 'playerFinderSetupHoldUnsupported';
      case 'setup-bag-boundary-unknown': return 'playerFinderSetupBagUnknown';
      case 'setup-residue-invalid': return 'playerFinderSetupResidueInvalid';
    }
  }

  function isTerminal(status: WorkspaceRuntimeStatus): boolean {
    return status === 'completed' || status === 'failed' || status === 'cancelled' || status === 'terminated';
  }

</script>

<svelte:window on:pointerdown={handleWindowPointerDown} />

<button
  bind:this={launcher}
  class="finder-launcher"
  type="button"
  aria-controls="player-finder-drawer"
  aria-expanded={open}
  aria-label={label('playerOpenFinder')}
  title={label('playerOpenFinder')}
  on:click={() => requestOpen(!open)}
>
  <Search size={19} strokeWidth={1.8} />
  <span>{label('playerFinder')}</span>
</button>

<aside
  bind:this={drawer}
  id="player-finder-drawer"
  class="finder-drawer"
  class:open
  aria-hidden={!open}
  inert={!open}
  aria-label={label('playerFinder')}
>
  <header>
    <div><Search size={18} strokeWidth={1.8} /><h2>{label('playerFinder')}</h2></div>
    <button
      bind:this={closeButton}
      type="button"
      aria-label={label('playerCloseFinder')}
      title={label('playerCloseFinder')}
      on:click={() => requestOpen(false)}
    ><X size={19} strokeWidth={1.8} /></button>
  </header>

  <div class="drawer-body">
    <nav aria-label={label('playerFinder')}>
      <button
        type="button"
        class:active={selected === 'pc'}
        aria-pressed={selected === 'pc'}
        disabled={active}
        on:click={() => selectFinder('pc')}
      >{label('pcSearch')}</button>
      {#if setupVisible}
        <button
          type="button"
          class:active={selected === 'setup'}
          aria-pressed={selected === 'setup'}
          disabled={active}
          on:click={() => selectFinder('setup')}
        >{label('setupFinder')}</button>
      {/if}
    </nav>

    <p class="finder-help">
      {label(selected === 'pc' ? 'playerPcFinderHelp' : 'playerSetupFinderHelp')}
    </p>

    {#if selected === 'pc'}
      <fieldset class="queue-mode" disabled={active}>
        <legend>{label('playerPcQueueMode')}</legend>
        <div class="mode-options">
          <label class:checked={pcQueueMode === 'queue-unknown'}>
            <input
              type="radio"
              name="player-pc-queue-mode"
              value="queue-unknown"
              bind:group={pcQueueMode}
              on:change={changePcQueueMode}
            />
            <span>{label('playerPcQueueUnknown')}</span>
          </label>
          <label class:checked={pcQueueMode === 'queue-based'}>
            <input
              type="radio"
              name="player-pc-queue-mode"
              value="queue-based"
              bind:group={pcQueueMode}
              on:change={changePcQueueMode}
            />
            <span>{label('playerPcQueueBased')}</span>
          </label>
        </div>
        <p>{label(pcQueueMode === 'queue-unknown'
          ? 'playerPcQueueUnknownHelp'
          : 'playerPcQueueBasedHelp')}</p>
      </fieldset>

      {#if pcQueueMode === 'queue-based'}
        <label class="range-toggle">
          <input
            type="checkbox"
            bind:checked={visibleRangeOnly}
            disabled={active}
            on:change={changePcQueueMode}
          />
          <span>
            <strong>{label('playerPcVisibleRange')}</strong>
            <small>{label('playerPcVisibleRangeHelp')}</small>
          </span>
        </label>
      {/if}
    {/if}

    {#if selectedIssue}
      <p class="finder-issue" role="status">{label(issueMessageKey(selectedIssue))}</p>
    {/if}

    <button
      class="run-button"
      class:cancel={active}
      type="button"
      disabled={!active && (!selectedPreparation || !selectedPreparation.ok)}
      on:click={() => active ? void cancel() : void runSelected()}
    >
      {#if active}<Square size={14} fill="currentColor" />{label('cancel')}
      {:else}<Search size={15} strokeWidth={2} />{label('playerFinderSearch')}{/if}
    </button>

    {#if resultKind === 'pc'}
      <div class="result-wrap">
        <PcSolverResult
          view={runtimeView}
          {language}
          {elapsedMs}
          targetLines={resultTargetLines}
          loadSolutionPage={runtime === 'web'
            ? (offset, limit, signal) => workerController.loadSolutionPage(offset, limit, signal)
            : null}
        />
      </div>
    {:else if resultKind === 'setup'}
      <div class="result-wrap">
        <SetupFinderResult
          view={runtimeView}
          {language}
          {elapsedMs}
          searchMode={setupSearchMode}
          pathDetails={{}}
          enablePathDetails={false}
        />
      </div>
    {/if}
  </div>
</aside>

<style>
  .finder-launcher {
    align-items: center;
    background: #fff;
    border: 1px solid #bfcac5;
    border-radius: 999px;
    box-shadow: 0 9px 24px rgba(24, 39, 34, .14);
    color: #31413b;
    cursor: pointer;
    display: inline-flex;
    font-size: 11px;
    font-weight: 800;
    gap: 7px;
    min-height: 44px;
    padding: 0 13px;
    position: fixed;
    right: 18px;
    top: 134px;
    z-index: 60;
  }
  .finder-launcher:hover { background: #e8f3f0; border-color: #78a9a1; color: #075f58; }
  .finder-launcher[aria-expanded='true'] { opacity: 0; pointer-events: none; visibility: hidden; }

  .finder-drawer {
    background: #fbfcfb;
    border-left: 1px solid #ccd6d1;
    box-shadow: -18px 0 42px rgba(22, 36, 31, .16);
    height: calc(100dvh - 70px);
    max-width: calc(100vw - 16px);
    overflow: hidden;
    pointer-events: none;
    position: fixed;
    right: 0;
    top: 70px;
    transform: translateX(102%);
    transition: transform 180ms ease, visibility 180ms;
    visibility: hidden;
    width: 620px;
    z-index: 63;
  }
  .finder-drawer.open { pointer-events: auto; transform: translateX(0); visibility: visible; }
  header, header > div, header button { align-items: center; display: flex; }
  header {
    background: #fff;
    border-bottom: 1px solid #d8dfdb;
    justify-content: space-between;
    min-height: 58px;
    padding: 0 14px 0 18px;
  }
  header > div { gap: 9px; }
  h2 { font-size: 14px; margin: 0; }
  header button { background: transparent; border: 0; border-radius: 5px; color: #53615b; cursor: pointer; height: 44px; justify-content: center; width: 44px; }
  header button:hover { background: #eef3f1; color: #17211e; }
  .drawer-body { height: calc(100% - 58px); overflow: auto; overscroll-behavior: contain; padding: 16px; }
  nav { display: grid; gap: 7px; grid-template-columns: repeat(2, minmax(0, 1fr)); }
  nav button {
    background: #f4f7f5;
    border: 1px solid #c8d2cd;
    border-radius: 5px;
    color: #53605b;
    cursor: pointer;
    font-size: 11px;
    font-weight: 800;
    min-height: 38px;
  }
  nav button.active { background: #dcece7; border-color: #16877d; color: #075f58; }
  nav button:disabled { cursor: default; opacity: .55; }
  .finder-help { color: #68736f; font-size: 11px; line-height: 1.55; margin: 12px 1px; }
  .queue-mode { border: 0; margin: 0 0 12px; min-width: 0; padding: 0; }
  .queue-mode legend { color: #35433e; font-size: 11px; font-weight: 800; margin-bottom: 7px; padding: 0; }
  .mode-options { display: grid; gap: 7px; grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .mode-options label {
    align-items: center;
    background: #f4f7f5;
    border: 1px solid #c8d2cd;
    border-radius: 5px;
    color: #53605b;
    cursor: pointer;
    display: flex;
    font-size: 11px;
    font-weight: 800;
    justify-content: center;
    min-height: 38px;
    padding: 0 8px;
  }
  .mode-options label.checked { background: #dcece7; border-color: #16877d; color: #075f58; }
  .mode-options input { height: 1px; opacity: 0; position: absolute; width: 1px; }
  .mode-options label:focus-within { outline: 2px solid #16877d; outline-offset: 2px; }
  .queue-mode:disabled .mode-options label { cursor: default; opacity: .55; }
  .queue-mode p { color: #68736f; font-size: 10px; line-height: 1.45; margin: 7px 1px 0; }
  .range-toggle {
    align-items: flex-start;
    background: #f7f9f8;
    border: 1px solid #d4dcd8;
    border-radius: 5px;
    cursor: pointer;
    display: flex;
    gap: 9px;
    margin: 0 0 12px;
    padding: 9px 10px;
  }
  .range-toggle input { margin: 2px 0 0; }
  .range-toggle span { display: grid; gap: 2px; }
  .range-toggle strong { color: #35433e; font-size: 11px; }
  .range-toggle small { color: #68736f; font-size: 10px; line-height: 1.4; }
  .finder-issue { background: #fff4ed; border: 1px solid #e6b29f; border-radius: 5px; color: #8b3d28; font-size: 11px; line-height: 1.45; margin: 10px 0; padding: 9px 10px; }
  .run-button {
    align-items: center;
    background: #0d7168;
    border: 1px solid #0d7168;
    border-radius: 5px;
    color: #fff;
    cursor: pointer;
    display: inline-flex;
    font-size: 12px;
    font-weight: 800;
    gap: 7px;
    justify-content: center;
    min-height: 40px;
    width: 100%;
  }
  .run-button.cancel { background: #fff; border-color: #bfc9c4; color: #4c5954; }
  .run-button:disabled { cursor: default; opacity: .42; }
  .result-wrap { border-top: 1px solid #d9dfdb; margin-top: 18px; min-width: 0; padding-top: 2px; }
  .result-wrap :global(.solver-result) { margin-left: 0; margin-right: 0; padding-bottom: 18px; padding-top: 16px; }
  .result-wrap :global(.result-workspace) { border-top: 0; padding: 16px 0; }
  .result-wrap :global(.result-body) { min-height: 160px; padding-top: 14px; }
  @media (prefers-reduced-motion: reduce) { .finder-drawer { transition: none; } }
  @media (max-width: 680px) {
    .finder-launcher { height: 44px; padding: 0; right: 12px; width: 44px; }
    .finder-launcher span { display: none; }
    .finder-drawer { width: min(560px, calc(100vw - 32px)); }
  }
</style>
