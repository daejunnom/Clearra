<script lang="ts">
  import { getContext, onDestroy, onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { componentMessage } from '../i18n/componentCatalog';
  import { readWorkspaceLanguage, persistWorkspaceLanguage } from './workspaceLanguagePreference';
  import { cancelDesktopJob, clearDesktopTerminalResult, desktopJobState, disposeDesktopJobPolling,
    resumeDesktopJobPolling, startDesktopJob, updateDesktopRequest } from '../stores';
  import { CPU_ONLY_RUNTIME_WARMUP_POLICY, HOST_CAPABILITY_SNAPSHOT_CONTEXT, automaticWorkerAuthority,
    clearWasmTerminalResult, sharedBrowserHostCapabilitySnapshot, updateWasmCommandText,
    wasmWorkerState, WasmTerminalWorkerController, type HostCapabilitySnapshot } from '../wasm';
  import { createRecoveryBuildRequest, recoveryBuildCommand, recoveryBuildDesktopRequest,
    resizeRecoveryBuild, validateRecoveryBuildRequest } from './recoveryBuildModel';
  import RecoveryBuildFields from './RecoveryBuildFields.svelte';
  import RecoveryBuildControls from './RecoveryBuildControls.svelte';
  import RecoveryBuildResult from './RecoveryBuildResult.svelte';
  import WorkspaceShell from './WorkspaceShell.svelte';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';
  import { workspaceViewFromDesktop, workspaceViewFromWasm } from './workspaceRuntime';
  export let workerFactory: (() => Worker) | null = null;
  export let runtime: 'web' | 'desktop' = 'web';
  const hostCapabilitySnapshot = getContext<HostCapabilitySnapshot>(HOST_CAPABILITY_SNAPSHOT_CONTEXT) ?? sharedBrowserHostCapabilitySnapshot();
  const workerController = new WasmTerminalWorkerController(workerFactory, hostCapabilitySnapshot);
  let request = createRecoveryBuildRequest();
  let language: WorkspaceLanguage = 'en';
  let disposed = false;
  let invalidHeightDraft = false;
  let elapsedMs = 0;
  let startedAt = 0;
  let timer: ReturnType<typeof setInterval> | null = null;
  $: workerController.setWorkerFactory(workerFactory);
  $: runtimeView = runtime === 'web' ? workspaceViewFromWasm($wasmWorkerState) : workspaceViewFromDesktop($desktopJobState);
  $: active = runtimeView.status === 'running' || runtimeView.status === 'cancelling';
  $: validation = [...validateRecoveryBuildRequest(request), ...(invalidHeightDraft ? ['height'] : [])];
  $: standard = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language,key);
  $: title = componentMessage(language,'boundaryRecovery');
  $: if (!active && timer !== null && ['completed','failed','cancelled','terminated'].includes(runtimeView.status)) stopTimer();
  onMount(() => {
    language = readWorkspaceLanguage();
    if (runtime === 'web') {
      clearWasmTerminalResult();
      const authority = automaticWorkerAuthority(hostCapabilitySnapshot, false);
      workerController.prewarm(authority.workersRequested, false, CPU_ONLY_RUNTIME_WARMUP_POLICY, authority);
    } else {
      clearDesktopTerminalResult();
      resumeDesktopJobPolling();
    }
    const handlePageHide = () => dispose();
    window.addEventListener('pagehide', handlePageHide);
    return () => window.removeEventListener('pagehide', handlePageHide);
  });
  onDestroy(dispose);

  function dispose() {
    if (disposed) return;
    disposed = true;
    if (runtime === 'web') {
      workerController.dispose();
      clearWasmTerminalResult();
      return;
    }
    const state = get(desktopJobState);
    if (state.jobId !== null || state.status === 'running' || state.status === 'cancelling') {
      void cancelDesktopJob();
    } else {
      disposeDesktopJobPolling();
      clearDesktopTerminalResult();
    }
  }


  function stopTimer() { if(timer!==null) {clearInterval(timer);timer=null;elapsedMs=performance.now()-startedAt;} }
  onDestroy(stopTimer);
  function setHeight(height:number) {
    invalidHeightDraft = !Number.isInteger(height) || height<1 || height>24;
    request = resizeRecoveryBuild(request,height);
  }
  async function run() {
    if(active || validation.length) return;
    stopTimer();elapsedMs=0;startedAt=performance.now();timer=setInterval(()=>elapsedMs=performance.now()-startedAt,100);
    if(runtime==='web') { updateWasmCommandText(recoveryBuildCommand(request));workerController.run(); }
    else { updateDesktopRequest(recoveryBuildDesktopRequest(request,language));await startDesktopJob(); }
  }
  async function cancel() { if(runtime==='web') workerController.cancel();else await cancelDesktopJob(); }
</script>
<svelte:head><title>{title} · Clearra</title></svelte:head>
<WorkspaceShell activeMode="recovery" {language} {active} statusLabel={standard(runtimeView.status)} workspaceLabel={title}
  dimensionLabel={standard('fieldHeight')} dimensionValue={request.height} dimensionMin={1} dimensionMax={24}
  cancelLabel={standard('cancel')} runLabel={standard('run')} runDisabled={validation.length>0}
  on:language={(event)=>{language=event.detail;persistWorkspaceLanguage(language);}}
  on:dimension={(event)=>setHeight(event.detail)} on:run={run} on:cancel={cancel}>
  <div slot="editor"><RecoveryBuildFields {request} {language} on:change={(event)=>{if(request.height!==event.detail.height)invalidHeightDraft=false;request=event.detail;}} /></div>
  <div slot="controls"><RecoveryBuildControls {request} {language} {validation} on:change={(event)=>request=event.detail} /></div>
  <div slot="result"><RecoveryBuildResult view={runtimeView} {language} {elapsedMs} /></div>
</WorkspaceShell>
