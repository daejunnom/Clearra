<script lang="ts">
  import { componentMessage, type ComponentMessageKey } from '../i18n/componentCatalog';
  import { readWorkspaceLanguage, persistWorkspaceLanguage } from './workspaceLanguagePreference';
  import { getContext, onDestroy, onMount } from 'svelte';

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
  import ProductFamilyResult from './ProductFamilyResult.svelte';
  import {
    buildSetupScoreCommand,
    createDefaultSetupScoreRequest,
    setupScoreRequestForDesktop,
    setupScoreValidationCodes,
    type SetupScoreRequest,
    type SetupScoreSourceKind,
    type SetupScoreValidationCode
  } from './setupScoreModel';
  import WorkspaceShell from './WorkspaceShell.svelte';
  import {
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
  let request = createDefaultSetupScoreRequest();
  let language: WorkspaceLanguage = 'en';
  let elapsedMs = 0;
  let runStartedAt = 0;
  let elapsedTimer: ReturnType<typeof setInterval> | null = null;

  $: workerController.setWorkerFactory(workerFactory);
  $: runtimeView = runtime === 'web'
    ? workspaceViewFromWasm($wasmWorkerState)
    : workspaceViewFromDesktop($desktopJobState);
  $: validationCodes = setupScoreValidationCodes(request);
  $: active = runtimeView.status === 'running' || runtimeView.status === 'cancelling';
  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);
  $: if (isTerminal(runtimeView.status) && elapsedTimer !== null) stopElapsedTimer();

  onMount(() => {
    language = readWorkspaceLanguage();
    request = {
      ...request,
      workers: automaticWorkerAuthority(
        hostCapabilitySnapshot,
        request.useAllLogicalProcessors
      ).workersEffective
    };
    if (runtime === 'web') prewarm();
    else resumeDesktopJobPolling();
    const handlePageHide = () => disposeWorkspace();
    window.addEventListener('pagehide', handlePageHide);
    return () => window.removeEventListener('pagehide', handlePageHide);
  });

  onDestroy(disposeWorkspace);

  function updateRequest(change: Partial<SetupScoreRequest>) {
    const next = { ...request, ...change };
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
    if (runtime === 'web' && useAllChanged) prewarm();
  }

  function prewarm() {
    workerController.prewarm(
      request.workers,
      false,
      CPU_ONLY_RUNTIME_WARMUP_POLICY,
      automaticWorkerAuthority(hostCapabilitySnapshot, request.useAllLogicalProcessors)
    );
  }

  async function run() {
    if (active || validationCodes.length) return;
    if (runtime === 'web') {
      updateWasmCommandText(buildSetupScoreCommand(request));
      if (workerController.run()) startElapsedTimer();
      return;
    }
    updateDesktopRequest(setupScoreRequestForDesktop(request, language));
    startElapsedTimer();
    await startDesktopJob();
  }

  async function cancel() {
    if (!active) return;
    if (runtime === 'web') workerController.cancel();
    else await cancelDesktopJob();
  }

  function setLanguage(next: WorkspaceLanguage) {
    language = next;
    persistWorkspaceLanguage(next);
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

  function isTerminal(status: WorkspaceRuntimeStatus): boolean {
    return ['completed', 'failed', 'cancelled', 'terminated'].includes(status);
  }

  function errorLabel(code: SetupScoreValidationCode): string {
    const messages: Record<SetupScoreValidationCode, ComponentMessageKey> = {
      document_invalid: 'enterAColoredCtk3FumenDocumentIn',
      setup_source_invalid: 'enterAValidSetupQueueOrPattern',
      solution_source_invalid: 'enterAContinuationQueueOrPatternOf',
      clear_height_invalid: 'clearHeightMustBeBetween1And',
      initial_b2b_invalid: 'initialB2bIsOutOfRange',
      max_patterns_invalid: 'maximumPatternsMustBeBetween1And',
      worker_count_invalid: 'workerCountMustBePositive'
    };
    return componentMessage(language, messages[code]);
  }
</script>

<svelte:head>
  <title>{componentMessage(language, 'setupScore')} · Clearra</title>
  <meta name="description" content={componentMessage(language, 'surfaceExactSetupAndContinuationScoreRanking')} />
</svelte:head>

<WorkspaceShell
  activeMode="setup-score"
  singlePanel
  {language}
  {active}
  statusLabel={label(runtimeView.status)}
  workspaceLabel={componentMessage(language, 'setupScore')}
  dimensionLabel={componentMessage(language, 'clearHeight')}
  dimensionValue={request.clearHeight}
  showDimension={false}
  cancelLabel={label('cancel')}
  runLabel={label('run')}
  runDisabled={validationCodes.length > 0}
  on:language={(event) => setLanguage(event.detail)}
  on:cancel={cancel}
  on:run={run}
>
  <section slot="controls" class="controls" aria-label={componentMessage(language, 'setupScoreInput')}>
    <div class="document-heading">
      <label>
        <span>{componentMessage(language, 'documentFormat')}</span>
        <select value={request.documentFormat} on:change={(event) => updateRequest({ documentFormat: (event.currentTarget as HTMLSelectElement).value as SetupScoreRequest['documentFormat'] })}>
          <option value="ctk3">CTK3</option>
          <option value="fumen">Fumen</option>
        </select>
      </label>
      <label>
        <span>{componentMessage(language, 'clearHeight')}</span>
        <input type="number" min="1" max="6" value={request.clearHeight} on:input={(event) => updateRequest({ clearHeight: Number((event.currentTarget as HTMLInputElement).value) })} />
      </label>
    </div>
    <label>
      <span>{componentMessage(language, 'coloredSolutionDocument')}</span>
      <textarea
        rows="4"
        spellcheck="false"
        value={request.document}
        placeholder={request.documentFormat === 'ctk3' ? 'ctk3_…' : 'v115@…'}
        on:input={(event) => updateRequest({ document: (event.currentTarget as HTMLTextAreaElement).value })}
      ></textarea>
    </label>
    <div class="source-grid">
      <fieldset>
        <legend>{componentMessage(language, 'setupSupply')}</legend>
        <select value={request.setupSourceKind} on:change={(event) => updateRequest({ setupSourceKind: (event.currentTarget as HTMLSelectElement).value as SetupScoreSourceKind })}>
          <option value="queue">{componentMessage(language, 'surfaceQueue')}</option>
          <option value="patterns">{componentMessage(language, 'surfacePatterns')}</option>
        </select>
        <input value={request.setupSource} spellcheck="false" on:input={(event) => updateRequest({ setupSource: (event.currentTarget as HTMLInputElement).value })} />
      </fieldset>
      <fieldset>
        <legend>{componentMessage(language, 'continuationSupply')}</legend>
        <select value={request.solutionSourceKind} on:change={(event) => updateRequest({ solutionSourceKind: (event.currentTarget as HTMLSelectElement).value as SetupScoreSourceKind })}>
          <option value="queue">{componentMessage(language, 'surfaceQueue')}</option>
          <option value="patterns">{componentMessage(language, 'surfacePatterns')}</option>
        </select>
        <input value={request.solutionSource} spellcheck="false" on:input={(event) => updateRequest({ solutionSource: (event.currentTarget as HTMLInputElement).value })} />
      </fieldset>
    </div>
    <div class="option-grid">
      <label><span>{componentMessage(language, 'scoreProfile')}</span><select value={request.scoreProfile} on:change={(event) => updateRequest({ scoreProfile: (event.currentTarget as HTMLSelectElement).value as SetupScoreRequest['scoreProfile'] })}><option value="tetrio">tetrio</option><option value="guideline">guideline</option><option value="jstris-ultra">jstris-ultra</option></select></label>
      <label><span>{componentMessage(language, 'initialB2b')}</span><input type="number" min="0" value={request.initialB2B} on:input={(event) => updateRequest({ initialB2B: Number((event.currentTarget as HTMLInputElement).value) })} /></label>
      <label><span>{componentMessage(language, 'rule')}</span><select value={request.rule} on:change={(event) => updateRequest({ rule: (event.currentTarget as HTMLSelectElement).value as SetupScoreRequest['rule'] })}><option value="srs-plus">srs-plus</option><option value="srs">srs</option><option value="srs-x">srs-x</option><option value="jstris-180">jstris-180</option><option value="no-kick">no-kick</option></select></label>
      <label><span>{componentMessage(language, 'maximumPatterns')}</span><input type="number" min="1" max="100000" value={request.maxPatterns} on:input={(event) => updateRequest({ maxPatterns: Number((event.currentTarget as HTMLInputElement).value) })} /></label>
      <label><span>{componentMessage(language, 'workers')}</span><input type="number" min="1" value={request.workers} disabled={request.useAllLogicalProcessors} on:input={(event) => updateRequest({ workers: Number((event.currentTarget as HTMLInputElement).value) })} /></label>
      <label class="check-row"><input type="checkbox" checked={request.holdEnabled} on:change={(event) => updateRequest({ holdEnabled: (event.currentTarget as HTMLInputElement).checked })} /><span>{componentMessage(language, 'enableSetupHold')}</span></label>
      <label class="check-row"><input type="checkbox" checked={request.useAllLogicalProcessors} on:change={(event) => updateRequest({ useAllLogicalProcessors: (event.currentTarget as HTMLInputElement).checked })} /><span>{componentMessage(language, 'allLogicalProcessors')}</span></label>
    </div>
    <p class="authority">{componentMessage(language, 'setupScoreIsCpuOnlyAndExposes')}</p>
    {#if validationCodes.length}
      <ul class="errors" aria-live="polite">{#each validationCodes as code}<li>{errorLabel(code)}</li>{/each}</ul>
    {/if}
  </section>
  <ProductFamilyResult
    slot="result"
    view={runtimeView}
    {language}
    {elapsedMs}
    capabilityLabel={componentMessage(language, 'setupScore')}
  />
</WorkspaceShell>

<style>
  .controls { display: grid; gap: 14px; }
  .document-heading, .source-grid, .option-grid { display: grid; gap: 10px; grid-template-columns: repeat(2, minmax(0, 1fr)); }
  label, fieldset { display: grid; gap: 6px; min-width: 0; }
  fieldset { border: 1px solid #dce3df; border-radius: 6px; grid-template-columns: 130px minmax(0, 1fr); margin: 0; padding: 10px; }
  legend, label > span { color: #53605b; font-size: 11px; font-weight: 720; }
  input, select, textarea { background: #fff; border: 1px solid #cbd3ce; border-radius: 5px; color: #26322e; font-size: 12px; min-width: 0; padding: 0 10px; width: 100%; }
  input, select { height: 39px; }
  textarea { line-height: 1.5; padding-bottom: 9px; padding-top: 9px; resize: vertical; }
  input:focus, select:focus, textarea:focus { border-color: #16877d; box-shadow: 0 0 0 3px #16877d1f; outline: 0; }
  .check-row { align-items: center; display: flex; gap: 8px; min-height: 39px; }
  .check-row input { height: 16px; margin: 0; width: 16px; }
  .authority { background: #f7f3ea; border: 1px solid #e3d8bd; border-radius: 5px; color: #725d29; font-size: 10px; line-height: 1.5; margin: 0; padding: 9px 10px; }
  .errors { background: #fff1f0; border: 1px solid #efc3be; border-radius: 5px; color: #8b2820; display: grid; font-size: 11px; gap: 4px; margin: 0; padding: 9px 12px 9px 28px; }
  @media (max-width: 700px) { .document-heading, .source-grid, .option-grid { grid-template-columns: 1fr; } fieldset { grid-template-columns: 1fr; } }
</style>
