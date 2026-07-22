<script lang="ts">
  import { TriangleAlert } from '@lucide/svelte';
  import { onDestroy, onMount } from 'svelte';

  import {
    cancelDesktopJob,
    desktopJobState,
    disposeDesktopJobPolling,
    resumeDesktopJobPolling,
    startDesktopJob,
    updateDesktopRequest
  } from '../stores';
  import {
    updateWasmCommandText,
    wasmWorkerState,
    WasmTerminalWorkerController
  } from '../wasm';
  import BoardEditor from './BoardEditor.svelte';
  import ResultWorkspace from './ResultWorkspace.svelte';
  import SearchControls from './SearchControls.svelte';
  import WorkspaceShell from './WorkspaceShell.svelte';
  import {
    buildWorkspaceCommand,
    clearCompletedRows,
    createDefaultWorkspaceRequest,
    defaultWorkerCount,
    trimBoardMask,
    workspaceRequestForDesktop,
    workspaceValidationCodes,
    type SolverWorkspaceRequest
  } from './solverWorkspaceModel';
  import {
    preferredWorkspaceLanguage,
    workspaceMessage,
    type WorkspaceLanguage
  } from './workspaceI18n';
  import {
    workspaceViewFromDesktop,
    workspaceViewFromWasm,
    type WorkspaceRuntimeStatus
  } from './workspaceRuntime';

  export let runtime: 'web' | 'desktop' = 'web';
  export let workerFactory: (() => Worker) | null = null;

  const workerController = new WasmTerminalWorkerController(workerFactory);
  let request = createDefaultWorkspaceRequest();
  let language: WorkspaceLanguage = 'en';
  let elapsedMs = 0;
  let runStartedAt = 0;
  let elapsedTimer: ReturnType<typeof setInterval> | null = null;
  let clearedRowsWarning = 0;
  let resultTargetLines = request.lines;

  $: workerController.setWorkerFactory(workerFactory);
  $: runtimeView = runtime === 'web'
    ? workspaceViewFromWasm($wasmWorkerState)
    : workspaceViewFromDesktop($desktopJobState);
  $: validationCodes = workspaceValidationCodes(request, runtime);
  $: active = runtimeView.status === 'running' || runtimeView.status === 'cancelling';
  $: label = (
    key: Parameters<typeof workspaceMessage>[1],
    values: Record<string, string | number> = {}
  ) => workspaceMessage(language, key, values);
  $: if (isTerminal(runtimeView.status) && elapsedTimer !== null) stopElapsedTimer();

  onMount(() => {
    language = preferredWorkspaceLanguage(
      localStorage.getItem('clearra-language') ?? navigator.language
    );
    const workers = defaultWorkerCount(navigator.hardwareConcurrency);
    request = {
      ...request,
      workers
    };
    if (runtime === 'web') workerController.prewarm(workers);
    if (runtime === 'desktop') resumeDesktopJobPolling();
  });

  onDestroy(() => {
    stopElapsedTimer();
    workerController.dispose();
    if (runtime === 'desktop') disposeDesktopJobPolling();
  });

  function setLanguage(next: WorkspaceLanguage) {
    language = next;
    localStorage.setItem('clearra-language', next);
  }

  function updateRequest(next: SolverWorkspaceRequest) {
    request = next;
  }

  function setLines(lines: number) {
    const bounded = Math.max(1, Math.min(6, Math.trunc(lines || 1)));
    clearedRowsWarning = 0;
    request = {
      ...request,
      lines: bounded,
      boardMask: trimBoardMask(request.boardMask, bounded)
    };
  }

  function setBoardMask(boardMask: bigint) {
    clearedRowsWarning = 0;
    request = {
      ...request,
      boardMask: trimBoardMask(boardMask, request.lines)
    };
  }

  async function run() {
    if (active || validationCodes.length) return;
    const normalized = clearCompletedRows(request.boardMask, request.lines);
    const executionRequest = normalized.clearedRows > 0
      ? {
          ...request,
          lines: normalized.remainingLines,
          boardMask: normalized.boardMask
        }
      : request;
    if (normalized.clearedRows > 0) request = executionRequest;
    clearedRowsWarning = normalized.clearedRows;
    resultTargetLines = executionRequest.lines;
    startElapsedTimer();
    if (runtime === 'web') {
      updateWasmCommandText(buildWorkspaceCommand(executionRequest));
      workerController.run();
      return;
    }
    updateDesktopRequest(workspaceRequestForDesktop(executionRequest, language));
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
    return status === 'completed' || status === 'failed' || status === 'cancelled';
  }

  function importBoard(event: CustomEvent<{ boardMask: bigint; lines: number }>) {
    const lines = Math.max(1, Math.min(6, event.detail.lines));
    clearedRowsWarning = 0;
    request = {
      ...request,
      lines,
      boardMask: trimBoardMask(event.detail.boardMask, lines)
    };
  }
</script>

<svelte:head>
  <title>Clearra</title>
  <meta name="description" content="Exact perfect-clear search workspace" />
</svelte:head>

<WorkspaceShell
    activeMode="pc"
    {language}
    {active}
    statusLabel={label(runtimeView.status)}
    runtimeLabel={label(runtime === 'web' ? 'runtimeWeb' : 'runtimeDesktop')}
    workspaceLabel={label('workspace')}
    dimensionLabel={label('targetLines')}
    dimensionValue={request.lines}
    dimensionMin={1}
    dimensionMax={6}
    cancelLabel={label('cancel')}
    runLabel={label('run')}
    runDisabled={validationCodes.length > 0}
    on:language={(event) => setLanguage(event.detail)}
    on:dimension={(event) => setLines(event.detail)}
    on:cancel={cancel}
    on:run={run}
  >
  <svelte:fragment slot="notice">
    {#if clearedRowsWarning > 0}
      <div class="field-warning" role="status" aria-live="polite">
        <TriangleAlert size={16} strokeWidth={1.9} />
        <span>{label('completedRowsCleared', { count: clearedRowsWarning })}</span>
      </div>
    {/if}
  </svelte:fragment>
  <BoardEditor slot="editor" {request} {language} on:change={(event) => setBoardMask(event.detail)} on:import={importBoard} />
  <SearchControls slot="controls" {request} {language} {validationCodes} on:change={(event) => updateRequest(event.detail)} />
  <ResultWorkspace slot="result" view={runtimeView} {language} {elapsedMs} targetLines={resultTargetLines} />
</WorkspaceShell>

<style>
  .field-warning {
    align-items: center;
    background: #fff4d6;
    border: 1px solid #dfbd68;
    border-radius: 5px;
    color: #654a0e;
    display: flex;
    font-size: 12px;
    font-weight: 650;
    gap: 8px;
    margin: 0 0 10px;
    padding: 9px 11px;
  }

</style>
