<script lang="ts">
  import { getContext, onDestroy, onMount } from 'svelte';
  import { get } from 'svelte/store';

  import {
    cancelDesktopJob,
    clearDesktopTerminalResult,
    desktopJobState,
    disposeDesktopJobPolling,
    resumeDesktopJobPolling,
    startDesktopJob,
    updateDesktopRequest
  } from '../stores';

  import {
    CPU_ONLY_RUNTIME_WARMUP_POLICY,
    HOST_CAPABILITY_SNAPSHOT_CONTEXT,
    automaticWorkerAuthority,
    clearWasmTerminalResult,
    sharedBrowserHostCapabilitySnapshot,
    updateWasmCommandText,
    wasmWorkerState,
    WasmTerminalWorkerController,
    type HostCapabilitySnapshot
  } from '../wasm';
  import ForwardSearchControls from './ForwardSearchControls.svelte';
  import ForwardSearchResult from './ForwardSearchResult.svelte';
  import WorkspaceBoardEditor from './WorkspaceBoardEditor.svelte';
  import WorkspaceShell from './WorkspaceShell.svelte';
  import {
    buildForwardSearchCommand,
    createDefaultForwardSearchRequest,
    forwardSourcePieceCount,
    forwardSearchRequestForDesktop,
    forwardSearchValidationCodes,
    trimForwardBoardMask,
    type ForwardDamageAggregation,
    type ForwardSearchRequest,
    type ForwardTool
  } from './forwardSearchModel';
  import { preferredWorkspaceLanguage, workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';
  import { workspaceViewFromDesktop, workspaceViewFromWasm, type WorkspaceRuntimeStatus } from './workspaceRuntime';

  export let tool: ForwardTool;
  export let workerFactory: (() => Worker) | null = null;
  export let runtime: 'web' | 'desktop' = 'web';

  const hostCapabilitySnapshot =
    getContext<HostCapabilitySnapshot>(HOST_CAPABILITY_SNAPSHOT_CONTEXT) ??
    sharedBrowserHostCapabilitySnapshot();
  const workerController = new WasmTerminalWorkerController(
    workerFactory,
    hostCapabilitySnapshot
  );
  let request = createDefaultForwardSearchRequest(tool);
  let language: WorkspaceLanguage = 'en';
  let elapsedMs = 0;
  let runStartedAt = 0;
  let elapsedTimer: ReturnType<typeof setInterval> | null = null;
  let resultHeight = request.height;
  let resultInitialBoardMask = request.boardMask;
  let resultDamageAggregation: ForwardDamageAggregation = request.damageAggregation;
  let resultMinimumDamage = request.minimumDamage;
  let workerCount = 1;
  let disposed = false;

  $: workerController.setWorkerFactory(workerFactory);
  $: workerAuthority = automaticWorkerAuthority(
    hostCapabilitySnapshot,
    request.useAllLogicalProcessors
  );
  $: if (request.tool !== tool) request = createDefaultForwardSearchRequest(tool);
  $: runtimeView = runtime === 'web'
    ? workspaceViewFromWasm($wasmWorkerState)
    : workspaceViewFromDesktop($desktopJobState);
  $: validationCodes = forwardSearchValidationCodes(request);
  $: active = runtimeView.status === 'running' || runtimeView.status === 'cancelling';
  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);
  $: if (isTerminal(runtimeView.status) && elapsedTimer !== null) stopElapsedTimer();

  onMount(() => {
    language = preferredWorkspaceLanguage(localStorage.getItem('clearra-language') ?? navigator.language);
    workerCount = automaticWorkerCount(request.useAllLogicalProcessors);
    if (runtime === 'web') {
      clearWasmTerminalResult();
      workerController.prewarm(
        workerCount,
        false,
        CPU_ONLY_RUNTIME_WARMUP_POLICY,
        automaticWorkerAuthority(
          hostCapabilitySnapshot,
          request.useAllLogicalProcessors
        )
      );
    } else {
      clearDesktopTerminalResult();
      resumeDesktopJobPolling();
    }
    const handlePageHide = () => disposeWorkspace();
    window.addEventListener('pagehide', handlePageHide);
    return () => window.removeEventListener('pagehide', handlePageHide);
  });

  onDestroy(disposeWorkspace);

  function disposeWorkspace() {
    if (disposed) return;
    disposed = true;
    stopElapsedTimer();
    if (runtime === 'desktop') {
      const desktopState = get(desktopJobState);
      const desktopJobActive =
        desktopState.jobId !== null ||
        desktopState.status === 'running' ||
        desktopState.status === 'cancelling';
      if (desktopJobActive) {
        // Keep the shared poller alive until the host confirms cancellation;
        // otherwise a route change can orphan the native job.
        void cancelDesktopJob();
      } else {
        disposeDesktopJobPolling();
        clearDesktopTerminalResult();
      }
      return;
    }
    workerController.dispose();
    clearWasmTerminalResult();
  }

  function setLanguage(next: WorkspaceLanguage) {
    language = next;
    localStorage.setItem('clearra-language', next);
  }

  function setHeight(value: number) {
    const height = Math.max(1, Math.min(24, Math.trunc(value || 1)));
    request = {
      ...request,
      height,
      boardMask: trimForwardBoardMask(request.boardMask, height)
    };
  }

  function setBoardMask(boardMask: bigint) {
    request = { ...request, boardMask: trimForwardBoardMask(boardMask, request.height) };
  }

  function importBoard(existingMask: bigint, height: number) {
    const nextHeight = Math.max(request.height, Math.max(1, Math.min(24, height)));
    request = {
      ...request,
      height: nextHeight,
      boardMask: trimForwardBoardMask(existingMask, nextHeight)
    };
  }

  function updateRequest(next: ForwardSearchRequest) {
    const useAllChanged = next.useAllLogicalProcessors !== request.useAllLogicalProcessors;
    if (useAllChanged) {
      workerCount = automaticWorkerCount(next.useAllLogicalProcessors);
      if (runtime === 'web') {
        workerController.prewarm(
          workerCount,
          false,
          CPU_ONLY_RUNTIME_WARMUP_POLICY,
          automaticWorkerAuthority(
            hostCapabilitySnapshot,
            next.useAllLogicalProcessors
          )
        );
      }
    }
    request = { ...next, tool };
  }

  function automaticWorkerCount(useAllLogicalProcessors: boolean): number {
    return automaticWorkerAuthority(
      hostCapabilitySnapshot,
      useAllLogicalProcessors
    ).workersEffective;
  }

  async function run() {
    if (active || validationCodes.length) return;
    resultHeight = request.height;
    resultInitialBoardMask = request.boardMask;
    resultDamageAggregation = request.damageAggregation;
    resultMinimumDamage = request.minimumDamage;
    if (runtime === 'web') {
      updateWasmCommandText(buildForwardSearchCommand(request, workerCount));
      if (workerController.run()) startElapsedTimer();
      return;
    }
    updateDesktopRequest(forwardSearchRequestForDesktop(request, language, workerCount));
    startElapsedTimer();
    await startDesktopJob();
  }

  async function cancel() {
    if (!active) return;
    if (runtime === 'web') workerController.cancel();
    else await cancelDesktopJob();
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
    if (elapsedTimer !== null) {
      clearInterval(elapsedTimer);
      elapsedTimer = null;
    }
    if (runStartedAt > 0) elapsedMs = performance.now() - runStartedAt;
  }

  function isTerminal(status: WorkspaceRuntimeStatus): boolean {
    return (
      status === 'completed' ||
      status === 'failed' ||
      status === 'cancelled' ||
      status === 'terminated'
    );
  }

</script>

<svelte:head>
  <title>{label(tool === 'damage' ? 'maximumDamage' : 'spinFinder')} · Clearra</title>
  <meta name="description" content="Exact forward damage and spin search workspace" />
</svelte:head>

<WorkspaceShell
    activeMode={tool}
    {language}
    {active}
    statusLabel={label(runtimeView.status)}
    workspaceLabel={label(tool === 'damage' ? 'maximumDamage' : 'spinFinder')}
    dimensionLabel={label('fieldHeight')}
    dimensionValue={request.height}
    dimensionMin={1}
    dimensionMax={24}
    cancelLabel={label('cancel')}
    runLabel={label('run')}
    runDisabled={validationCodes.length > 0}
    on:language={(event) => setLanguage(event.detail)}
    on:dimension={(event) => setHeight(event.detail)}
    on:cancel={cancel}
    on:run={run}
  >
  <WorkspaceBoardEditor
    slot="editor"
    mode="forward"
    height={request.height}
    existingMask={request.boardMask}
    targetMask={0n}
    piecesNeeded={forwardSourcePieceCount(request)}
    {language}
    on:change={(event) => setBoardMask(event.detail.existingMask)}
    on:import={(event) => importBoard(event.detail.existingMask, event.detail.height)}
  />
  <ForwardSearchControls slot="controls" {request} {language} {validationCodes} {workerAuthority} on:change={(event) => updateRequest(event.detail)} />
  <ForwardSearchResult
    slot="result"
    report={runtimeView.searchReport}
    diagnostics={runtimeView.diagnostics}
    status={runtimeView.status}
    error={runtimeView.error ?? ''}
    {elapsedMs}
    progressLabel={runtimeView.progressLabel}
    progressDone={runtimeView.progressDone}
    progressTotal={runtimeView.progressTotal}
    progressTelemetry={runtimeView.progressTelemetry}
    forwardPatternDone={runtimeView.forwardPatternDone}
    forwardPatternTotal={runtimeView.forwardPatternTotal}
    {language}
    height={resultHeight}
    initialBoardMask={resultInitialBoardMask}
    damageAggregation={resultDamageAggregation}
    minimumDamage={resultMinimumDamage}
    {tool}
  />
</WorkspaceShell>
