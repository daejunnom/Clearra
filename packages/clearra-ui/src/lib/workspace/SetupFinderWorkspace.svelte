<script lang="ts">
  import { onDestroy, onMount } from 'svelte';

  import {
    buildWasmCommandRequest,
    ensureWasmWorkerOwnerId,
    postRunCommand,
    terminateOwnedWasmWorker,
    updateWasmCommandText,
    wasmWorkerState,
    WasmTerminalWorkerController,
    type ClearraWasmWorkerEvent
  } from '../wasm';
  import SetupFinderControls from './SetupFinderControls.svelte';
  import SetupFinderResult from './SetupFinderResult.svelte';
  import {
    buildSetupFinderCommand,
    buildSetupPathDetailCommand,
    createDefaultSetupFinderRequest,
    setupCycle,
    setupFinderValidationCodes,
    setupPathDetailKey,
    type SetupFinderRequest,
    type SetupPathDetailRequest,
    type SetupPathDetailState
  } from './setupFinderModel';
  import WorkspaceShell from './WorkspaceShell.svelte';
  import {
    preferredWorkspaceLanguage,
    workspaceMessage,
    type WorkspaceLanguage
  } from './workspaceI18n';
  import { workspaceViewFromWasm, type WorkspaceRuntimeStatus } from './workspaceRuntime';
  import { defaultWorkerCount } from './solverWorkspaceModel';

  export let workerFactory: (() => Worker) | null = null;

  const workerController = new WasmTerminalWorkerController(workerFactory);
  let request = createDefaultSetupFinderRequest();
  let resultRequest: SetupFinderRequest | null = null;
  let pathDetails: Record<string, SetupPathDetailState> = {};
  let detailWorker: Worker | null = null;
  let detailWorkerBusy = false;
  let activeDetailKey: string | null = null;
  let detailGeneration = 0;
  let language: WorkspaceLanguage = 'en';
  let elapsedMs = 0;
  let runStartedAt = 0;
  let elapsedTimer: ReturnType<typeof setInterval> | null = null;
  let prewarmWorkerCount = 1;

  $: workerController.setWorkerFactory(workerFactory);
  $: runtimeView = workspaceViewFromWasm($wasmWorkerState);
  $: validationCodes = setupFinderValidationCodes(request);
  $: active = runtimeView.status === 'running' || runtimeView.status === 'cancelling';
  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);
  $: if (isTerminal(runtimeView.status) && elapsedTimer !== null) stopElapsedTimer();

  onMount(() => {
    language = preferredWorkspaceLanguage(
      localStorage.getItem('clearra-language') ?? navigator.language
    );
    prewarmWorkerCount = defaultWorkerCount(navigator.hardwareConcurrency);
    workerController.prewarm(prewarmWorkerCount, request.tablebaseEnabled);
    const handlePageHide = () => disposeWorkspace();
    window.addEventListener('pagehide', handlePageHide);
    return () => window.removeEventListener('pagehide', handlePageHide);
  });

  onDestroy(disposeWorkspace);

  function disposeWorkspace() {
    stopElapsedTimer();
    disposeDetailWorker();
    workerController.dispose();
  }

  function setLanguage(next: WorkspaceLanguage) {
    language = next;
    localStorage.setItem('clearra-language', next);
  }

  function updateRequest(next: SetupFinderRequest) {
    const tablebaseChanged = next.tablebaseEnabled !== request.tablebaseEnabled;
    request = setupCycle(next.remaining) === 7
      ? next
      : { ...next, allowPostCycleBorrow: false };
    if (tablebaseChanged) {
      workerController.prewarm(prewarmWorkerCount, request.tablebaseEnabled);
    }
  }

  function run() {
    if (active || validationCodes.length) return;
    disposeDetailWorker();
    pathDetails = {};
    resultRequest = { ...request };
    startElapsedTimer();
    updateWasmCommandText(buildSetupFinderCommand(request));
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

  function loadSetupPaths(detail: SetupPathDetailRequest) {
    const key = setupPathDetailKey(detail);
    const existing = pathDetails[key];
    if (existing?.status === 'loading' || existing?.status === 'complete') return;
    if (!workerFactory || !resultRequest) {
      updatePathDetail(key, {
        status: 'failed',
        paths: [],
        complete: false,
        error: label('pathDetailUnavailable')
      });
      return;
    }

    if (detailWorkerBusy) {
      if (activeDetailKey) {
        updatePathDetail(activeDetailKey, {
          status: 'failed',
          paths: [],
          complete: false,
          error: label('cancelled')
        });
      }
      disposeDetailWorker();
    }
    const generation = ++detailGeneration;
    activeDetailKey = key;
    detailWorkerBusy = true;
    updatePathDetail(key, {
      status: 'loading',
      paths: [],
      complete: false,
      error: null
    });

    const worker = detailWorker ?? workerController.takeIdleWorker() ?? workerFactory();
    detailWorker = worker;
    worker.onmessage = (message: MessageEvent<ClearraWasmWorkerEvent>) => {
      if (detailWorker !== worker || generation !== detailGeneration) return;
      const event = message.data;
      if (!('event' in event)) return;
      if (event.event === 'final_response') {
        const condition = event.search_report?.setup_report?.hold_conditions.find(
          (candidate) => candidate.condition_id === detail.conditionId
        );
        const candidate = condition?.candidates.find(
          (candidate) => candidate.setup_id === detail.setupId
        );
        if (
          event.response.status === 'success' &&
          candidate?.solution_paths_complete === true
        ) {
          updatePathDetail(key, {
            status: 'complete',
            paths: candidate.solution_paths ?? [],
            complete: true,
            error: null
          });
          finishDetailWorkerRequest(worker);
        } else {
          updatePathDetail(key, {
            status: 'failed',
            paths: [],
            complete: false,
            error:
              event.response.diagnostics.map((diagnostic) => diagnostic.message).join('\n') ||
              label('pathDetailFailed')
          });
          releaseDetailWorker(worker);
        }
      } else if (event.event === 'failed') {
        updatePathDetail(key, {
          status: 'failed',
          paths: [],
          complete: false,
          error:
            event.diagnostics.diagnostics.map((diagnostic) => diagnostic.message).join('\n') ||
            label('pathDetailFailed')
        });
        releaseDetailWorker(worker);
      } else if (event.event === 'cancelled') {
        updatePathDetail(key, {
          status: 'failed',
          paths: [],
          complete: false,
          error: label('cancelled')
        });
        releaseDetailWorker(worker, 'owner-disposed');
      } else if (event.event === 'terminated') {
        updatePathDetail(key, {
          status: 'failed',
          paths: [],
          complete: false,
          error:
            event.diagnostics.diagnostics.map((diagnostic) => diagnostic.message).join('\n') ||
            label('pathDetailFailed')
        });
        releaseDetailWorker(worker, 'worker-failure');
      }
    };
    worker.onerror = (event) => {
      event.preventDefault();
      if (detailWorker !== worker || generation !== detailGeneration) return;
      updatePathDetail(key, {
        status: 'failed',
        paths: [],
        complete: false,
        error: event.message || label('pathDetailFailed')
      });
      releaseDetailWorker(worker);
    };
    worker.onmessageerror = () => {
      if (detailWorker !== worker || generation !== detailGeneration) return;
      updatePathDetail(key, {
        status: 'failed',
        paths: [],
        complete: false,
        error: label('pathDetailFailed')
      });
      releaseDetailWorker(worker);
    };
    try {
      postRunCommand(
        worker,
        buildWasmCommandRequest({
          commandText: buildSetupPathDetailCommand(resultRequest, detail)
        }),
        1,
        resultRequest.tablebaseEnabled,
        ensureWasmWorkerOwnerId(worker)
      );
    } catch (error) {
      updatePathDetail(key, {
        status: 'failed',
        paths: [],
        complete: false,
        error: error instanceof Error ? error.message : label('pathDetailFailed')
      });
      releaseDetailWorker(worker);
    }
  }

  function updatePathDetail(key: string, state: SetupPathDetailState) {
    pathDetails = { ...pathDetails, [key]: state };
  }

  function disposeDetailWorker() {
    detailGeneration += 1;
    detailWorkerBusy = false;
    activeDetailKey = null;
    const worker = detailWorker;
    detailWorker = null;
    if (!worker) return;
    worker.onmessage = null;
    worker.onerror = null;
    worker.onmessageerror = null;
    terminateOwnedWasmWorker(worker, 'owner-disposed');
  }

  function releaseDetailWorker(
    worker: Worker,
    reason: 'owner-disposed' | 'worker-failure' = 'worker-failure'
  ) {
    if (detailWorker !== worker) return;
    detailWorkerBusy = false;
    activeDetailKey = null;
    detailWorker = null;
    worker.onmessage = null;
    worker.onerror = null;
    worker.onmessageerror = null;
    try {
      worker.postMessage({ type: 'dispose_runtime' });
    } catch {}
    terminateOwnedWasmWorker(worker, reason);
  }

  function finishDetailWorkerRequest(worker: Worker) {
    if (detailWorker !== worker) return;
    detailWorkerBusy = false;
    activeDetailKey = null;
  }
</script>

<svelte:head>
  <title>{label('setupFinder')} · Clearra</title>
  <meta name="description" content="Exact 4-line perfect-clear setup finder" />
</svelte:head>

<WorkspaceShell
  activeMode="setup"
  singlePanel
  {language}
  {active}
  statusLabel={label(runtimeView.status)}
  workspaceLabel={label('setupFinder')}
  dimensionLabel={label('targetLines')}
  dimensionValue={4}
  showDimension={false}
  cancelLabel={label('cancel')}
  runLabel={label('run')}
  runDisabled={validationCodes.length > 0}
  on:language={(event) => setLanguage(event.detail)}
  on:cancel={cancel}
  on:run={run}
>
  <SetupFinderControls
    slot="controls"
    {request}
    {language}
    {validationCodes}
    tablebaseStatus={$wasmWorkerState.tablebaseWarmup.status}
    tablebaseByteLength={$wasmWorkerState.tablebaseWarmup.byteLength}
    on:change={(event) => updateRequest(event.detail)}
  />
  <SetupFinderResult
    slot="result"
    view={runtimeView}
    {language}
    {elapsedMs}
    {pathDetails}
    on:loadPaths={(event) => loadSetupPaths(event.detail)}
  />
</WorkspaceShell>
