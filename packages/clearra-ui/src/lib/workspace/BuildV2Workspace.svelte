<script lang="ts">
  import { getContext, onDestroy, onMount } from 'svelte';

  import {
    loadNextProductPage as loadNextDesktopProductPage,
    loadProductMemberPage as loadDesktopProductMemberPage,
    releaseProductPages as releaseDesktopProductPages
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
    clearWasmTerminalResult,
    sharedBrowserHostCapabilitySnapshot,
    updateWasmCommandText,
    wasmWorkerState,
    WasmTerminalWorkerController,
    type HostCapabilitySnapshot
  } from '../wasm';
  import BuildV2Controls from './BuildV2Controls.svelte';
  import BuildV2Result from './BuildV2Result.svelte';
  import BuildV2SourceEditor from './BuildV2SourceEditor.svelte';
  import WorkspaceShell from './WorkspaceShell.svelte';
  import {
    buildV2Command,
    buildV2RequestForDesktop,
    buildV2SourceKind,
    buildV2ValidationCodes,
    createDefaultBuildV2Request,
    normalizeBuildV2Request,
    trimBuildV2Mask,
    updateBuildV2Draft,
    type BuildV2Request
  } from './buildV2Model';
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

  export let workerFactory: (() => Worker) | null = null;
  export let runtime: 'web' | 'desktop' = 'web';

  const hostCapabilitySnapshot =
    getContext<HostCapabilitySnapshot>(HOST_CAPABILITY_SNAPSHOT_CONTEXT) ??
    sharedBrowserHostCapabilitySnapshot();
  const workerController = new WasmTerminalWorkerController(workerFactory, hostCapabilitySnapshot);
  let request = createDefaultBuildV2Request();
  let language: WorkspaceLanguage = 'en';
  let elapsedMs = 0;
  let runStartedAt = 0;
  let elapsedTimer: ReturnType<typeof setInterval> | null = null;

  $: workerController.setWorkerFactory(workerFactory);
  $: workerAuthority = automaticWorkerAuthority(
    hostCapabilitySnapshot,
    request.useAllLogicalProcessors
  );
  $: runtimeView = runtime === 'web'
    ? workspaceViewFromWasm($wasmWorkerState)
    : workspaceViewFromDesktop($desktopJobState);
  $: validationCodes = buildV2ValidationCodes(request);
  $: sourceKind = buildV2SourceKind(request.capability);
  $: active = runtimeView.status === 'running' || runtimeView.status === 'cancelling';
  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);
  $: if (isTerminal(runtimeView.status) && elapsedTimer !== null) stopElapsedTimer();

  onMount(() => {
    language = preferredWorkspaceLanguage(
      localStorage.getItem('clearra-language') ?? navigator.language
    );
    request = { ...request, workers: workerAuthority.workersEffective };
    if (runtime === 'web') {
      workerController.prewarm(
        request.workers,
        false,
        CPU_ONLY_RUNTIME_WARMUP_POLICY,
        automaticWorkerAuthority(hostCapabilitySnapshot, request.useAllLogicalProcessors)
      );
    } else {
      resumeDesktopJobPolling();
    }
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
    const bounded = Math.max(1, Math.min(24, Math.trunc(height || 1)));
    request = {
      ...request,
      height: bounded,
      baseMask: trimBuildV2Mask(request.baseMask, bounded),
      targetMask: trimBuildV2Mask(request.targetMask, bounded)
    };
  }

  function updateRequest(change: Partial<BuildV2Request>) {
    const next = updateBuildV2Draft(request, change);
    const useAllChanged = next.useAllLogicalProcessors !== request.useAllLogicalProcessors;
    request = useAllChanged
      ? {
          ...next,
          workers: automaticWorkerAuthority(
            hostCapabilitySnapshot,
            next.useAllLogicalProcessors
          ).workersEffective
        }
      : next;
    if (runtime === 'web' && useAllChanged) {
      workerController.prewarm(
        request.workers,
        false,
        CPU_ONLY_RUNTIME_WARMUP_POLICY,
        automaticWorkerAuthority(hostCapabilitySnapshot, request.useAllLogicalProcessors)
      );
    }
  }

  async function run() {
    if (active || validationCodes.length) return;
    const executionRequest = normalizeBuildV2Request(request);
    if (runtime === 'web') {
      updateWasmCommandText(buildV2Command(executionRequest));
      if (workerController.run()) startElapsedTimer();
      return;
    }
    updateDesktopRequest(buildV2RequestForDesktop(executionRequest, language));
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
  <title>{language === 'ko' ? 'Build 도구' : 'Build tools'} · Clearra</title>
  <meta name="description" content="Typed Build target production, supplied-solution evaluation, exact portfolio paging, and score-only result inspection" />
</svelte:head>

<WorkspaceShell
  activeMode="build"
  {language}
  {active}
  statusLabel={label(runtimeView.status)}
  workspaceLabel={language === 'ko' ? 'Build 도구' : 'Build tools'}
  dimensionLabel={label('fieldHeight')}
  dimensionValue={request.height}
  dimensionMin={1}
  dimensionMax={24}
  showDimension={sourceKind === 'mask'}
  cancelLabel={label('cancel')}
  runLabel={label('run')}
  runDisabled={validationCodes.length > 0}
  on:language={(event) => setLanguage(event.detail)}
  on:dimension={(event) => setHeight(event.detail)}
  on:cancel={cancel}
  on:run={run}
>
  <BuildV2SourceEditor
    slot="editor"
    {request}
    {language}
    on:change={(event) => updateRequest(event.detail)}
  />
  <BuildV2Controls
    slot="controls"
    {request}
    {language}
    {validationCodes}
    on:change={(event) => updateRequest(event.detail)}
  />
  <BuildV2Result
    slot="result"
    view={runtimeView}
    {language}
    {elapsedMs}
    loadNextProductPage={runtime === 'web'
      ? (signal) => workerController.loadNextProductPage(signal)
      : (signal) => loadNextDesktopProductPage(10_000, signal)}
    loadProductMemberPage={runtime === 'web'
      ? (outerPageNumber, memberPageNumber, signal) =>
          workerController.loadProductMemberPage(outerPageNumber, memberPageNumber, signal)
      : (outerPageNumber, memberPageNumber, signal) =>
          loadDesktopProductMemberPage(outerPageNumber, memberPageNumber, signal)}
    releaseProductPages={runtime === 'web'
      ? () => workerController.releaseProductPages()
      : () => releaseDesktopProductPages()}
  />
</WorkspaceShell>
