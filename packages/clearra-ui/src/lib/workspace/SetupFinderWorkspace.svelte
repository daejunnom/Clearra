<script lang="ts">
  import { getContext, onDestroy, onMount } from 'svelte';

  import {
    cancelJob as cancelDesktopDetailJob,
    getJobEvents as getDesktopDetailJobEvents,
    startJob as startDesktopDetailJob,
    type ClearraDesktopJobEvent
  } from '../host';

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
    buildWasmCommandRequest,
    clearWasmTerminalResult,
    ensureWasmWorkerOwnerId,
    postRunCommand,
    resolveWorkerAuthority,
    sharedBrowserHostCapabilitySnapshot,
    terminateOwnedWasmWorker,
    updateWasmCommandText,
    wasmWorkerState,
    WasmTerminalWorkerController,
    type ClearraWasmWorkerEvent,
    type HostCapabilitySnapshot
  } from '../wasm';
  import SetupFinderControls from './SetupFinderControls.svelte';
  import SetupFinderResult from './SetupFinderResult.svelte';
  import {
    buildSetupFinderCommand,
    buildSetupPathDetailCommand,
    createDefaultSetupFinderRequest,
    setupCycle,
    setupFinderValidationCodes,
    setupFinderRequestForDesktop,
    setupPathDetailKey,
    type SetupFinderRequest,
    type SetupPathDetailRequest,
    type SetupPathDetailState
  } from './setupFinderModel';
  import { cancelSetupPathDetail } from './setupPathDetailState';
  import WorkspaceShell from './WorkspaceShell.svelte';
  import {
    preferredWorkspaceLanguage,
    workspaceMessage,
    type WorkspaceLanguage
  } from './workspaceI18n';
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
  let request = createDefaultSetupFinderRequest();
  let resultRequest: SetupFinderRequest | null = null;
  let pathDetails: Record<string, SetupPathDetailState> = {};
  let detailWorker: Worker | null = null;
  let detailWorkerBusy = false;
  let activeDetailKey: string | null = null;
  let detailGeneration = 0;
  let desktopDetailJobId: number | null = null;
  let desktopDetailPending: DesktopDetailRequest | null = null;
  let desktopDetailPump: Promise<void> | null = null;
  let language: WorkspaceLanguage = 'en';
  let elapsedMs = 0;
  let runStartedAt = 0;
  let elapsedTimer: ReturnType<typeof setInterval> | null = null;
  let prewarmWorkerCount = 1;

  $: workerController.setWorkerFactory(workerFactory);
  $: workerAuthority = automaticWorkerAuthority(
    hostCapabilitySnapshot,
    request.useAllLogicalProcessors
  );
  $: runtimeView = runtime === 'web'
    ? workspaceViewFromWasm($wasmWorkerState)
    : workspaceViewFromDesktop($desktopJobState);
  $: validationCodes = setupFinderValidationCodes(request);
  $: mainJobActive = runtimeView.status === 'running' || runtimeView.status === 'cancelling';
  $: active = mainJobActive || detailWorkerBusy;
  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);
  $: if (isTerminal(runtimeView.status) && elapsedTimer !== null) stopElapsedTimer();

  onMount(() => {
    language = preferredWorkspaceLanguage(
      localStorage.getItem('clearra-language') ?? navigator.language
    );
    prewarmWorkerCount = automaticWorkerCount(request.useAllLogicalProcessors);
    if (runtime === 'web') {
      workerController.prewarm(
        prewarmWorkerCount,
        request.tablebaseEnabled,
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
    if (runtime === 'desktop') void stopDesktopDetail(false);
    else disposeDetailWorker();
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

  function updateRequest(next: SetupFinderRequest) {
    const tablebaseChanged = next.tablebaseEnabled !== request.tablebaseEnabled;
    const useAllChanged = next.useAllLogicalProcessors !== request.useAllLogicalProcessors;
    if (useAllChanged) {
      prewarmWorkerCount = automaticWorkerCount(next.useAllLogicalProcessors);
    }
    request = setupCycle(next.remaining) === 7
      ? next
      : { ...next, allowPostCycleBorrow: false };
    if (runtime === 'web' && (tablebaseChanged || useAllChanged)) {
      workerController.prewarm(
        prewarmWorkerCount,
        request.tablebaseEnabled,
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

  async function run() {
    if (active || validationCodes.length) return;
    disposeDetailWorker();
    pathDetails = {};
    resultRequest = { ...request };
    if (runtime === 'web') {
      updateWasmCommandText(buildSetupFinderCommand(request, prewarmWorkerCount));
      if (workerController.run()) startElapsedTimer();
      return;
    }
    startElapsedTimer();
    updateDesktopRequest(setupFinderRequestForDesktop(request, language, prewarmWorkerCount));
    await startDesktopJob();
  }

  async function cancel() {
    if (!active) return;
    if (detailWorkerBusy) {
      if (runtime === 'desktop') await stopDesktopDetail(true);
      else cancelWebDetail();
      return;
    }
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

  function loadSetupPaths(detail: SetupPathDetailRequest) {
    const key = setupPathDetailKey(detail);
    const existing = pathDetails[key];
    if (existing?.status === 'loading' || existing?.status === 'complete') return;
    if (!resultRequest) {
      updatePathDetail(key, {
        status: 'failed',
        paths: [],
        complete: false,
        error: label('pathDetailUnavailable')
      });
      return;
    }
    if (runtime === 'desktop') {
      queueDesktopDetail(key, detail, resultRequest);
      return;
    }
    if (!workerFactory) {
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
          commandText: buildSetupPathDetailCommand(resultRequest, detail, 1)
        }),
        1,
        resultRequest.tablebaseEnabled,
        ensureWasmWorkerOwnerId(worker),
        {
          hostCapabilitySnapshot,
          workerAuthority: resolveWorkerAuthority(hostCapabilitySnapshot, 1),
          warmupPolicy: CPU_ONLY_RUNTIME_WARMUP_POLICY
        }
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

  type DesktopDetailRequest = {
    key: string;
    detail: SetupPathDetailRequest;
    request: SetupFinderRequest;
    generation: number;
  };

  function queueDesktopDetail(
    key: string,
    detail: SetupPathDetailRequest,
    sourceRequest: SetupFinderRequest
  ) {
    if (activeDetailKey && activeDetailKey !== key) {
      updatePathDetail(activeDetailKey, {
        status: 'failed',
        paths: [],
        complete: false,
        error: label('cancelled')
      });
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
    desktopDetailPending = {
      key,
      detail,
      request: { ...sourceRequest },
      generation
    };
    if (desktopDetailJobId !== null) {
      void cancelDesktopDetailJob(desktopDetailJobId).catch(() => undefined);
    }
    ensureDesktopDetailPump();
  }

  function ensureDesktopDetailPump() {
    if (desktopDetailPump !== null) return;
    desktopDetailPump = pumpDesktopDetails().finally(() => {
      desktopDetailPump = null;
      if (desktopDetailPending !== null) {
        ensureDesktopDetailPump();
      } else if (desktopDetailJobId === null) {
        detailWorkerBusy = false;
        activeDetailKey = null;
      }
    });
  }

  async function pumpDesktopDetails() {
    while (desktopDetailPending !== null) {
      const pending = desktopDetailPending;
      desktopDetailPending = null;
      await runDesktopDetail(pending);
    }
  }

  async function runDesktopDetail(pending: DesktopDetailRequest) {
    if (pending.generation !== detailGeneration) return;
    let jobId: number;
    try {
      jobId = await startDesktopDetailJob(
        setupFinderRequestForDesktop(pending.request, language, 1, pending.detail)
      );
      desktopDetailJobId = jobId;
    } catch (error) {
      failDesktopDetail(pending, error instanceof Error ? error.message : String(error));
      return;
    }

    let cancellationSent = false;
    while (desktopDetailJobId === jobId) {
      if (pending.generation !== detailGeneration && !cancellationSent) {
        cancellationSent = true;
        try {
          await cancelDesktopDetailJob(jobId);
        } catch {}
      }
      let events: ClearraDesktopJobEvent[];
      try {
        events = await getDesktopDetailJobEvents(jobId);
      } catch (error) {
        desktopDetailJobId = null;
        failDesktopDetail(pending, error instanceof Error ? error.message : String(error));
        return;
      }
      const terminal = events.find((event) =>
        event.event === 'completed' || event.event === 'failed' || event.event === 'cancelled'
      );
      if (terminal) {
        desktopDetailJobId = null;
        if (pending.generation === detailGeneration) {
          applyDesktopDetailTerminal(pending, terminal);
        }
        return;
      }
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
  }

  function applyDesktopDetailTerminal(
    pending: DesktopDetailRequest,
    event: ClearraDesktopJobEvent
  ) {
    if (event.event === 'completed') {
      const condition = event.search_report?.setup_report?.hold_conditions.find(
        (candidate) => candidate.condition_id === pending.detail.conditionId
      );
      const candidate = condition?.candidates.find(
        (candidate) => candidate.setup_id === pending.detail.setupId
      );
      if (event.response?.status === 'success' && candidate?.solution_paths_complete === true) {
        updatePathDetail(pending.key, {
          status: 'complete',
          paths: candidate.solution_paths ?? [],
          complete: true,
          error: null
        });
        return;
      }
      failDesktopDetail(
        pending,
        event.response?.diagnostics.map((diagnostic) => diagnostic.message).join('\n') ||
          label('pathDetailFailed')
      );
      return;
    }
    failDesktopDetail(
      pending,
      event.event === 'cancelled' ? label('cancelled') : event.code ?? label('pathDetailFailed')
    );
  }

  function failDesktopDetail(pending: DesktopDetailRequest, error: string) {
    if (pending.generation !== detailGeneration) return;
    updatePathDetail(pending.key, {
      status: 'failed',
      paths: [],
      complete: false,
      error
    });
  }

  async function stopDesktopDetail(showCancelled: boolean) {
    const activeKey = activeDetailKey;
    detailGeneration += 1;
    desktopDetailPending = null;
    if (showCancelled && activeKey) {
      updatePathDetail(activeKey, {
        status: 'failed',
        paths: [],
        complete: false,
        error: label('cancelled')
      });
    }
    const jobId = desktopDetailJobId;
    if (jobId !== null) {
      try {
        await cancelDesktopDetailJob(jobId);
      } catch {}
    }
    if (desktopDetailPump !== null) {
      try {
        await desktopDetailPump;
      } catch {}
    }
    desktopDetailJobId = null;
    detailWorkerBusy = false;
    activeDetailKey = null;
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

  function cancelWebDetail() {
    pathDetails = cancelSetupPathDetail(pathDetails, activeDetailKey, label('cancelled'));
    disposeDetailWorker();
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
    tablebaseStatus={runtime === 'web' ? $wasmWorkerState.tablebaseWarmup.status : 'disabled'}
    tablebaseByteLength={$wasmWorkerState.tablebaseWarmup.byteLength}
    {workerAuthority}
    on:change={(event) => updateRequest(event.detail)}
  />
  <SetupFinderResult
    slot="result"
    view={runtimeView}
    {language}
    {elapsedMs}
    searchMode={resultRequest?.searchMode ?? request.searchMode}
    {pathDetails}
    on:loadPaths={(event) => loadSetupPaths(event.detail)}
  />
</WorkspaceShell>
