<script lang="ts">
  import { onDestroy, onMount } from 'svelte';

  import { updateWasmCommandText, wasmWorkerState, WasmTerminalWorkerController } from '../wasm';
  import SetupFinderControls from './SetupFinderControls.svelte';
  import SetupFinderResult from './SetupFinderResult.svelte';
  import {
    buildSetupFinderCommand,
    createDefaultSetupFinderRequest,
    setupCycle,
    setupFinderValidationCodes,
    type SetupFinderRequest
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
  const boardCells = Array.from({ length: 40 }, (_, index) => index);
  let request = createDefaultSetupFinderRequest();
  let language: WorkspaceLanguage = 'en';
  let elapsedMs = 0;
  let runStartedAt = 0;
  let elapsedTimer: ReturnType<typeof setInterval> | null = null;

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
    workerController.prewarm(defaultWorkerCount(navigator.hardwareConcurrency));
  });

  onDestroy(() => {
    stopElapsedTimer();
    workerController.dispose();
  });

  function setLanguage(next: WorkspaceLanguage) {
    language = next;
    localStorage.setItem('clearra-language', next);
  }

  function updateRequest(next: SetupFinderRequest) {
    request = setupCycle(next.remaining) === 7
      ? next
      : { ...next, allowPostCycleBorrow: false };
  }

  function run() {
    if (active || validationCodes.length) return;
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
    return status === 'completed' || status === 'failed' || status === 'cancelled';
  }
</script>

<svelte:head>
  <title>{label('setupFinder')} · Clearra</title>
  <meta name="description" content="Exact 4-line perfect-clear setup finder" />
</svelte:head>

<WorkspaceShell
  activeMode="setup"
  {language}
  {active}
  statusLabel={label(runtimeView.status)}
  runtimeLabel={label('runtimeWeb')}
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
  <section slot="editor" class="fixed-target" aria-label={label('pcTarget')}>
    <header>
      <span>{label('pcTarget')}</span>
      <strong>4L · 10×4</strong>
    </header>
    <div class="target-frame">
      <div class="target-board" role="img" aria-label={label('emptyFourLineField')}>
        {#each boardCells as _}<span></span>{/each}
      </div>
    </div>
    <dl>
      <div><dt>{label('fieldState')}</dt><dd>{label('empty')}</dd></div>
      <div><dt>{label('targetCells')}</dt><dd>40</dd></div>
      <div><dt>{label('completionPieces')}</dt><dd>10</dd></div>
      <div><dt>{label('lineClearPolicy')}</dt><dd>{label('lineClearInverseExact')}</dd></div>
    </dl>
  </section>
  <SetupFinderControls
    slot="controls"
    {request}
    {language}
    {validationCodes}
    on:change={(event) => updateRequest(event.detail)}
  />
  <SetupFinderResult slot="result" view={runtimeView} {language} {elapsedMs} />
</WorkspaceShell>

<style>
  .fixed-target header { display: grid; gap: 4px; margin-bottom: 14px; }
  .fixed-target header span { color: #68736f; font-size: 11px; font-weight: 700; }
  .fixed-target header strong { color: #17211e; font-size: 16px; }
  .target-frame { background: #101817; border: 1px solid #253330; border-radius: 6px; margin: 0 auto; max-width: 650px; padding: 16px; }
  .target-board { aspect-ratio: 2.5; display: grid; grid-template-columns: repeat(10, 1fr); grid-template-rows: repeat(4, 1fr); }
  .target-board span { background: #1e2927; box-shadow: inset 0 0 0 1px rgba(216, 226, 222, .2); }
  dl { background: #eef2ef; display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); margin: 12px 0 0; }
  dl div { align-items: center; border-bottom: 1px solid #dce3df; display: flex; justify-content: space-between; padding: 9px 11px; }
  dt { color: #6b7672; font-size: 10px; }
  dd { color: #263b35; font-size: 11px; font-weight: 750; margin: 0; text-align: right; }
  @media (max-width: 560px) { dl { grid-template-columns: 1fr; } }
</style>
