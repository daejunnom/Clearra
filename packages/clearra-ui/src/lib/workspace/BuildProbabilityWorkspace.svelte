<script lang="ts">
  import { TriangleAlert } from '@lucide/svelte';
  import { onDestroy, onMount, tick } from 'svelte';

  import { updateWasmCommandText, wasmWorkerState, WasmTerminalWorkerController } from '../wasm';
  import BuildProbabilityBoardEditor from './BuildProbabilityBoardEditor.svelte';
  import BuildProbabilityControls from './BuildProbabilityControls.svelte';
  import BuildProbabilityResult from './BuildProbabilityResult.svelte';
  import WorkspaceShell from './WorkspaceShell.svelte';
  import {
    buildProbabilityCommand,
    buildProbabilityValidationCodes,
    createDefaultBuildProbabilityRequest,
    trimBuildProbabilityMask,
    trimBuildProbabilityRequest,
    type BuildProbabilityRequest
  } from './buildProbabilityModel';
  import { defaultWorkerCount } from './solverWorkspaceModel';
  import { preferredWorkspaceLanguage, workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';
  import { workspaceViewFromWasm, type WorkspaceRuntimeStatus } from './workspaceRuntime';

  export let workerFactory: (() => Worker) | null = null;

  const workerController = new WasmTerminalWorkerController(workerFactory);
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
  $: runtimeView = workspaceViewFromWasm($wasmWorkerState);
  $: validationCodes = buildProbabilityValidationCodes(request);
  $: active = runtimeView.status === 'running' || runtimeView.status === 'cancelling';
  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);
  $: if (isTerminal(runtimeView.status) && elapsedTimer !== null) stopElapsedTimer();

  onMount(() => {
    language = preferredWorkspaceLanguage(localStorage.getItem('clearra-language') ?? navigator.language);
    const workers = defaultWorkerCount(navigator.hardwareConcurrency);
    request = { ...request, workers };
    workerController.prewarm(workers);
    const handlePageHide = () => disposeWorkspace();
    window.addEventListener('pagehide', handlePageHide);
    return () => window.removeEventListener('pagehide', handlePageHide);
  });

  onDestroy(disposeWorkspace);

  function disposeWorkspace() {
    stopElapsedTimer();
    workerController.dispose();
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

  function run() {
    if (active || validationCodes.length) return;
    continuationApplied = false;
    resultHeight = request.height;
    resultExistingMask = request.existingMask;
    resultTargetMask = request.targetMask;
    resultAggregation = request.aggregation;
    startElapsedTimer();
    updateWasmCommandText(buildProbabilityCommand(request));
    workerController.run();
  }

  function cancel() {
    if (active) workerController.cancel();
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
  <meta name="description" content="Exact fixed-field build probability workspace" />
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
  <BuildProbabilityControls slot="controls" {request} {language} {validationCodes} on:change={(event) => (request = event.detail)} />
  <BuildProbabilityResult
    slot="result"
    view={runtimeView}
    {language}
    {elapsedMs}
    height={resultHeight}
    existingMask={resultExistingMask}
    targetMask={resultTargetMask}
    aggregation={resultAggregation}
    on:continue={(event) => continueFromCompletedBuild(event.detail.existingMask, event.detail.height)}
  />
</WorkspaceShell>

<style>
  .continuation-notice { background: #e8f3ef; border: 1px solid #c6ddd5; border-radius: 5px; color: #155d55; font-size: 12px; font-weight: 700; margin: 0 0 10px; padding: 10px 12px; }
  .tiling-warning { align-items: center; color: #9a4d35; display: flex; font-size: 11px; font-weight: 700; gap: 7px; min-width: 0; }
</style>
