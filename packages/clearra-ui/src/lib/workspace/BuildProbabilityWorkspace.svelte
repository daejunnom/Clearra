<script lang="ts">
  import { TriangleAlert } from '@lucide/svelte';
  import { getContext, onDestroy, onMount, tick } from 'svelte';

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
  import BuildProbabilityBoardEditor from './BuildProbabilityBoardEditor.svelte';
  import BuildProbabilityControls from './BuildProbabilityControls.svelte';
  import BuildProbabilityResult from './BuildProbabilityResult.svelte';
  import WorkspaceShell from './WorkspaceShell.svelte';
  import {
    buildProbabilityCommand,
    buildProbabilityRequestForDesktop,
    buildProbabilityValidationCodes,
    createDefaultBuildProbabilityRequest,
    normalizeBuildProbabilityRequest,
    trimBuildProbabilityMask,
    trimBuildProbabilityRequest,
    type BuildProbabilityRequest
  } from './buildProbabilityModel';
  import { preferredWorkspaceLanguage, workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';
  import { workspaceViewFromDesktop, workspaceViewFromWasm, type WorkspaceRuntimeStatus } from './workspaceRuntime';

  export let workerFactory: (() => Worker) | null = null;
  export let runtime: 'web' | 'desktop' = 'web';

  const hostCapabilitySnapshot =
    getContext<HostCapabilitySnapshot>(HOST_CAPABILITY_SNAPSHOT_CONTEXT) ??
    sharedBrowserHostCapabilitySnapshot();
  const workerController = new WasmTerminalWorkerController(
    workerFactory,
    hostCapabilitySnapshot
  );
  let request = createDefaultBuildProbabilityRequest();
  let language: WorkspaceLanguage = 'en';
  let elapsedMs = 0;
  let runStartedAt = 0;
  let elapsedTimer: ReturnType<typeof setInterval> | null = null;
  let resultHeight = request.height;
  let resultExistingMask = request.existingMask;
  let resultTargetMask = request.targetMask;
  let resultAggregation = request.aggregation;
  let continuationApplied = false;
  let workspaceShell: { scrollWorkspaceIntoView: () => void } | null = null;

  $: workerController.setWorkerFactory(workerFactory);
  $: workerAuthority = automaticWorkerAuthority(
    hostCapabilitySnapshot,
    request.useAllLogicalProcessors
  );
  $: runtimeView = runtime === 'web'
    ? workspaceViewFromWasm($wasmWorkerState)
    : workspaceViewFromDesktop($desktopJobState);
  $: validationCodes = buildProbabilityValidationCodes(request);
  $: active = runtimeView.status === 'running' || runtimeView.status === 'cancelling';
  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);
  $: if (isTerminal(runtimeView.status) && elapsedTimer !== null) stopElapsedTimer();

  onMount(() => {
    language = preferredWorkspaceLanguage(localStorage.getItem('clearra-language') ?? navigator.language);
    const workers = automaticWorkerCount(request.useAllLogicalProcessors);
    request = { ...request, workers };
    if (runtime === 'web') {
      workerController.prewarm(
        workers,
        false,
        CPU_ONLY_RUNTIME_WARMUP_POLICY,
        automaticWorkerAuthority(
          hostCapabilitySnapshot,
          request.useAllLogicalProcessors
        )
      );
    }
    else resumeDesktopJobPolling();
    const handlePageHide = () => disposeWorkspace();
    window.addEventListener('pagehide', handlePageHide);
    return () => window.removeEventListener('pagehide', handlePageHide);
  });

  onDestroy(disposeWorkspace);

  function disposeWorkspace() {
    stopElapsedTimer();
    workerController.dispose();
    if (runtime === 'desktop') {
      disposeDesktopJobPolling();
      clearDesktopTerminalResult();
    } else {
      clearWasmTerminalResult();
    }
  }

  function setLanguage(next: WorkspaceLanguage) {
    language = next;
    localStorage.setItem('clearra-language', next);
  }

  function setHeight(height: number) {
    continuationApplied = false;
    request = trimBuildProbabilityRequest(request, height);
  }

  function setMasks(existingMask: bigint, targetMask: bigint) {
    continuationApplied = false;
    const existing = trimBuildProbabilityMask(existingMask, request.height);
    request = {
      ...request,
      existingMask: existing,
      targetMask: trimBuildProbabilityMask(targetMask, request.height) & ~existing
    };
  }

  function importExisting(existingMask: bigint, height: number) {
    continuationApplied = false;
    const next = trimBuildProbabilityRequest(request, Math.max(request.height, height));
    const existing = trimBuildProbabilityMask(existingMask, next.height);
    request = { ...next, existingMask: existing, targetMask: next.targetMask & ~existing };
  }

  async function continueFromCompletedBuild(existingMask: bigint, height: number) {
    const next = trimBuildProbabilityRequest(request, height);
    request = {
      ...next,
      existingMask: trimBuildProbabilityMask(existingMask, next.height),
      targetMask: 0n
    };
    continuationApplied = true;
    await tick();
    workspaceShell?.scrollWorkspaceIntoView();
  }

  async function run() {
    if (active || validationCodes.length) return;
    continuationApplied = false;
    const executionRequest = normalizeBuildProbabilityRequest(request);
    resultHeight = executionRequest.height;
    resultExistingMask = executionRequest.existingMask;
    resultTargetMask = executionRequest.targetMask;
    resultAggregation = executionRequest.aggregation;
    if (runtime === 'web') {
      updateWasmCommandText(buildProbabilityCommand(executionRequest));
      if (workerController.run()) startElapsedTimer();
      return;
    }
    startElapsedTimer();
    updateDesktopRequest(buildProbabilityRequestForDesktop(executionRequest, language));
    await startDesktopJob();
  }

  function updateRequest(next: BuildProbabilityRequest) {
    const useAllChanged = next.useAllLogicalProcessors !== request.useAllLogicalProcessors;
    request = useAllChanged
      ? { ...next, workers: automaticWorkerCount(next.useAllLogicalProcessors) }
      : next;
    if (runtime === 'web' && useAllChanged) {
      workerController.prewarm(
        request.workers,
        false,
        CPU_ONLY_RUNTIME_WARMUP_POLICY,
        automaticWorkerAuthority(
          hostCapabilitySnapshot,
          request.useAllLogicalProcessors
        )
      );
    }
  }

  function automaticWorkerCount(useAllLogicalProcessors: boolean): number {
    return automaticWorkerAuthority(
      hostCapabilitySnapshot,
      useAllLogicalProcessors
    ).workersEffective;
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
  <title>{label('buildProbability')} · Clearra</title>
  <meta name="description" content="Exact full-future/oracle build probability workspace; finesse queue-information policies are reported separately" />
</svelte:head>

<WorkspaceShell
    bind:this={workspaceShell}
    activeMode="build-probability"
    {language}
    {active}
    statusLabel={label(runtimeView.status)}
    workspaceLabel={label('buildProbability')}
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
  <svelte:fragment slot="action-warning">
    {#if request.aggregation === 'tiling'}
      <div class="tiling-warning" role="status">
        <TriangleAlert size={16} strokeWidth={1.9} />
        <span>{label('tilingOnlyWarning')}</span>
      </div>
    {/if}
  </svelte:fragment>
  <svelte:fragment slot="notice">
    {#if continuationApplied}
      <p class="continuation-notice" aria-live="polite">{label('nextBuildBaseApplied')}</p>
    {/if}
  </svelte:fragment>
  <BuildProbabilityBoardEditor
    slot="editor"
    {request}
    {language}
    on:change={(event) => setMasks(event.detail.existingMask, event.detail.targetMask)}
    on:import={(event) => importExisting(event.detail.existingMask, event.detail.height)}
  />
  <BuildProbabilityControls slot="controls" {request} {language} {validationCodes} {workerAuthority} on:change={(event) => updateRequest(event.detail)} />
  <BuildProbabilityResult
    slot="result"
    view={runtimeView}
    {language}
    {elapsedMs}
    height={resultHeight}
    existingMask={resultExistingMask}
    targetMask={resultTargetMask}
    aggregation={resultAggregation}
    loadSolutionPage={runtime === 'web'
      ? (offset, limit, signal) => workerController.loadSolutionPage(offset, limit, signal)
      : null}
    on:continue={(event) => continueFromCompletedBuild(event.detail.existingMask, event.detail.height)}
  />
</WorkspaceShell>

<style>
  .continuation-notice { background: #e8f3ef; border: 1px solid #c6ddd5; border-radius: 5px; color: #155d55; font-size: 12px; font-weight: 700; margin: 0 0 10px; padding: 10px 12px; }
  .tiling-warning { align-items: center; color: #9a4d35; display: flex; font-size: 11px; font-weight: 700; gap: 7px; min-width: 0; }
</style>
