<script lang="ts">
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
  $: korean = language === 'ko';

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
    const messages: Record<SetupScoreValidationCode, readonly [string, string]> = {
      document_invalid: ['선택한 형식의 색상 CTK3/Fumen 문서를 입력하세요.', 'Enter a colored CTK3/Fumen document in the selected format.'],
      setup_source_invalid: ['Setup queue 또는 pattern을 확인하세요.', 'Enter a valid setup queue or pattern.'],
      solution_source_invalid: ['16조각 이하의 continuation queue 또는 pattern을 입력하세요.', 'Enter a continuation queue or pattern of at most 16 pieces.'],
      clear_height_invalid: ['Clear 높이는 1..6이어야 합니다.', 'Clear height must be between 1 and 6.'],
      initial_b2b_invalid: ['초기 B2B 범위를 확인하세요.', 'Initial B2B is out of range.'],
      max_patterns_invalid: ['최대 pattern 수는 1..100000이어야 합니다.', 'Maximum patterns must be between 1 and 100000.'],
      worker_count_invalid: ['Worker 수는 1 이상이어야 합니다.', 'Worker count must be positive.']
    };
    return messages[code][korean ? 0 : 1];
  }
</script>

<svelte:head>
  <title>{korean ? 'Setup 점수' : 'Setup score'} · Clearra</title>
  <meta name="description" content="Exact setup and continuation score ranking" />
</svelte:head>

<WorkspaceShell
  activeMode="setup-score"
  singlePanel
  {language}
  {active}
  statusLabel={label(runtimeView.status)}
  workspaceLabel={korean ? 'Setup 점수' : 'Setup score'}
  dimensionLabel={korean ? 'Clear 높이' : 'Clear height'}
  dimensionValue={request.clearHeight}
  showDimension={false}
  cancelLabel={label('cancel')}
  runLabel={label('run')}
  runDisabled={validationCodes.length > 0}
  on:language={(event) => setLanguage(event.detail)}
  on:cancel={cancel}
  on:run={run}
>
  <section slot="controls" class="controls" aria-label={korean ? 'Setup 점수 입력' : 'Setup score input'}>
    <div class="document-heading">
      <label>
        <span>{korean ? '문서 형식' : 'Document format'}</span>
        <select value={request.documentFormat} on:change={(event) => updateRequest({ documentFormat: (event.currentTarget as HTMLSelectElement).value as SetupScoreRequest['documentFormat'] })}>
          <option value="ctk3">CTK3</option>
          <option value="fumen">Fumen</option>
        </select>
      </label>
      <label>
        <span>{korean ? 'Clear 높이' : 'Clear height'}</span>
        <input type="number" min="1" max="6" value={request.clearHeight} on:input={(event) => updateRequest({ clearHeight: Number((event.currentTarget as HTMLInputElement).value) })} />
      </label>
    </div>
    <label>
      <span>{korean ? '색상 해법 문서' : 'Colored solution document'}</span>
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
        <legend>{korean ? 'Setup 공급' : 'Setup supply'}</legend>
        <select value={request.setupSourceKind} on:change={(event) => updateRequest({ setupSourceKind: (event.currentTarget as HTMLSelectElement).value as SetupScoreSourceKind })}>
          <option value="queue">queue</option>
          <option value="patterns">patterns</option>
        </select>
        <input value={request.setupSource} spellcheck="false" on:input={(event) => updateRequest({ setupSource: (event.currentTarget as HTMLInputElement).value })} />
      </fieldset>
      <fieldset>
        <legend>{korean ? '연속 해법 공급' : 'Continuation supply'}</legend>
        <select value={request.solutionSourceKind} on:change={(event) => updateRequest({ solutionSourceKind: (event.currentTarget as HTMLSelectElement).value as SetupScoreSourceKind })}>
          <option value="queue">queue</option>
          <option value="patterns">patterns</option>
        </select>
        <input value={request.solutionSource} spellcheck="false" on:input={(event) => updateRequest({ solutionSource: (event.currentTarget as HTMLInputElement).value })} />
      </fieldset>
    </div>
    <div class="option-grid">
      <label><span>{korean ? '점수 프로필' : 'Score profile'}</span><select value={request.scoreProfile} on:change={(event) => updateRequest({ scoreProfile: (event.currentTarget as HTMLSelectElement).value as SetupScoreRequest['scoreProfile'] })}><option value="tetrio">tetrio</option><option value="guideline">guideline</option><option value="jstris-ultra">jstris-ultra</option></select></label>
      <label><span>{korean ? '초기 B2B' : 'Initial B2B'}</span><input type="number" min="0" value={request.initialB2B} on:input={(event) => updateRequest({ initialB2B: Number((event.currentTarget as HTMLInputElement).value) })} /></label>
      <label><span>{korean ? '규칙' : 'Rule'}</span><select value={request.rule} on:change={(event) => updateRequest({ rule: (event.currentTarget as HTMLSelectElement).value as SetupScoreRequest['rule'] })}><option value="srs-plus">srs-plus</option><option value="srs">srs</option><option value="srs-x">srs-x</option><option value="jstris-180">jstris-180</option><option value="no-kick">no-kick</option></select></label>
      <label><span>{korean ? '최대 pattern' : 'Maximum patterns'}</span><input type="number" min="1" max="100000" value={request.maxPatterns} on:input={(event) => updateRequest({ maxPatterns: Number((event.currentTarget as HTMLInputElement).value) })} /></label>
      <label><span>{korean ? 'Worker 수' : 'Workers'}</span><input type="number" min="1" value={request.workers} disabled={request.useAllLogicalProcessors} on:input={(event) => updateRequest({ workers: Number((event.currentTarget as HTMLInputElement).value) })} /></label>
      <label class="check-row"><input type="checkbox" checked={request.holdEnabled} on:change={(event) => updateRequest({ holdEnabled: (event.currentTarget as HTMLInputElement).checked })} /><span>{korean ? 'Setup Hold 사용' : 'Enable setup hold'}</span></label>
      <label class="check-row"><input type="checkbox" checked={request.useAllLogicalProcessors} on:change={(event) => updateRequest({ useAllLogicalProcessors: (event.currentTarget as HTMLInputElement).checked })} /><span>{korean ? '모든 논리 프로세서' : 'All logical processors'}</span></label>
    </div>
    <p class="authority">{korean ? 'Setup score는 CPU 전용이며 fallback과 memory/GPU 옵션을 노출하지 않습니다. 동일 score는 안정적인 순서로 표시하며 attack을 혼합하지 않습니다.' : 'Setup score is CPU-only and exposes no fallback, memory, or GPU option. Equal scores use a stable display order without mixing attack.'}</p>
    {#if validationCodes.length}
      <ul class="errors" aria-live="polite">{#each validationCodes as code}<li>{errorLabel(code)}</li>{/each}</ul>
    {/if}
  </section>
  <ProductFamilyResult
    slot="result"
    view={runtimeView}
    {language}
    {elapsedMs}
    capabilityLabel={korean ? 'Setup 점수' : 'Setup score'}
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
