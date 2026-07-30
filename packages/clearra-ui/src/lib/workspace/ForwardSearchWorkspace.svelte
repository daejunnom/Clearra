<script lang="ts">
  import { onDestroy, onMount } from 'svelte';

  import { updateWasmCommandText, wasmWorkerState, WasmTerminalWorkerController } from '../wasm';
  import ForwardSearchControls from './ForwardSearchControls.svelte';
  import ForwardSearchResult from './ForwardSearchResult.svelte';
  import WorkspaceBoardEditor from './WorkspaceBoardEditor.svelte';
  import WorkspaceShell from './WorkspaceShell.svelte';
  import {
    buildForwardSearchCommand,
    createDefaultForwardSearchRequest,
    forwardSourcePieceCount,
    forwardSearchValidationCodes,
    trimForwardBoardMask,
    type ForwardDamageAggregation,
    type ForwardSearchRequest,
    type ForwardTool
  } from './forwardSearchModel';
  import { defaultWorkerCount } from './solverWorkspaceModel';
  import { preferredWorkspaceLanguage, workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';
  import { workspaceViewFromWasm, type WorkspaceRuntimeStatus } from './workspaceRuntime';

  export let tool: ForwardTool;
  export let workerFactory: (() => Worker) | null = null;

  const workerController = new WasmTerminalWorkerController(workerFactory);
  let request = createDefaultForwardSearchRequest(tool);
  let language: WorkspaceLanguage = 'en';
  let elapsedMs = 0;
  let runStartedAt = 0;
  let elapsedTimer: ReturnType<typeof setInterval> | null = null;
  let resultHeight = request.height;
  let resultInitialBoardMask = request.boardMask;
  let resultDamageAggregation: ForwardDamageAggregation = request.damageAggregation;
  let resultMinimumDamage = request.minimumDamage;

  $: workerController.setWorkerFactory(workerFactory);
  $: if (request.tool !== tool) request = createDefaultForwardSearchRequest(tool);
  $: runtimeView = workspaceViewFromWasm($wasmWorkerState);
  $: validationCodes = forwardSearchValidationCodes(request);
  $: active = runtimeView.status === 'running' || runtimeView.status === 'cancelling';
  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);
  $: if (isTerminal(runtimeView.status) && elapsedTimer !== null) stopElapsedTimer();

  onMount(() => {
    language = preferredWorkspaceLanguage(localStorage.getItem('clearra-language') ?? navigator.language);
    workerController.prewarm(defaultWorkerCount(navigator.hardwareConcurrency));
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
    request = { ...next, tool };
  }

  function run() {
    if (active || validationCodes.length) return;
    resultHeight = request.height;
    resultInitialBoardMask = request.boardMask;
    resultDamageAggregation = request.damageAggregation;
    resultMinimumDamage = request.minimumDamage;
    startElapsedTimer();
    updateWasmCommandText(buildForwardSearchCommand(request));
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
  <ForwardSearchControls slot="controls" {request} {language} {validationCodes} on:change={(event) => updateRequest(event.detail)} />
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
