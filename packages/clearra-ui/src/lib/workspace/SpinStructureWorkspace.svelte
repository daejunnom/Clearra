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
  import ProductFamilyResult from './ProductFamilyResult.svelte';
  import {
    buildSpinStructureCommand,
    createDefaultSpinStructureRequest,
    normalizedSpinInventory,
    spinStructureRequestForDesktop,
    spinStructureValidationCodes,
    type SpinStructureRequest,
    type SpinStructureValidationCode
  } from './spinStructureModel';
  import WorkspaceShell from './WorkspaceShell.svelte';
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
  let request = createDefaultSpinStructureRequest();
  let language: WorkspaceLanguage = 'en';
  let elapsedMs = 0;
  let runStartedAt = 0;
  let elapsedTimer: ReturnType<typeof setInterval> | null = null;

  $: workerController.setWorkerFactory(workerFactory);
  $: runtimeView = runtime === 'web'
    ? workspaceViewFromWasm($wasmWorkerState)
    : workspaceViewFromDesktop($desktopJobState);
  $: validationCodes = spinStructureValidationCodes(request);
  $: active = runtimeView.status === 'running' || runtimeView.status === 'cancelling';
  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);
  $: korean = language === 'ko';
  $: if (isTerminal(runtimeView.status) && elapsedTimer !== null) stopElapsedTimer();

  onMount(() => {
    language = preferredWorkspaceLanguage(
      localStorage.getItem('clearra-language') ?? navigator.language
    );
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

  function updateRequest(change: Partial<SpinStructureRequest>) {
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

  function setHeight(value: number) {
    const visibleHeight = Math.max(4, Math.min(24, Math.trunc(value || 4)));
    updateRequest({
      visibleHeight,
      fillTop: Math.min(request.fillTop, visibleHeight),
      fillBottom: Math.min(request.fillBottom, Math.max(0, visibleHeight - 1))
    });
  }

  function setInventory(value: string) {
    const normalized = normalizedSpinInventory(value);
    updateRequest({
      inventory: value,
      maxPlacements: Math.min(request.maxPlacements, Math.max(1, normalized.length))
    });
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
      updateWasmCommandText(buildSpinStructureCommand(request));
      if (workerController.run()) startElapsedTimer();
      return;
    }
    updateDesktopRequest(spinStructureRequestForDesktop(request, language));
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
    localStorage.setItem('clearra-language', next);
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

  function errorLabel(code: SpinStructureValidationCode): string {
    const messages: Record<SpinStructureValidationCode, readonly [string, string]> = {
      board_mask_invalid: ['Board mask v1은 소문자 60자리 16진수여야 합니다.', 'Board mask v1 must be 60 lowercase hexadecimal digits.'],
      height_invalid: ['높이는 4..24여야 합니다.', 'Height must be between 4 and 24.'],
      board_outside_height: ['Board mask에 선택 높이 밖의 셀이 있습니다.', 'The board mask has cells outside the selected height.'],
      inventory_invalid: ['1..255개의 I,O,T,S,Z,J,L inventory를 입력하세요.', 'Enter an inventory of 1..255 I,O,T,S,Z,J,L pieces.'],
      fill_window_invalid: ['Fill 범위는 0 ≤ bottom < top ≤ height여야 합니다.', 'The fill range must satisfy 0 ≤ bottom < top ≤ height.'],
      max_placements_invalid: ['최대 배치는 1 이상이며 inventory 수를 넘을 수 없습니다.', 'Maximum placements must be positive and no greater than the inventory.'],
      max_patterns_invalid: ['최대 pattern 수는 1..100000이어야 합니다.', 'Maximum patterns must be between 1 and 100000.'],
      final_piece_invalid: ['보장 마지막 조각은 inventory와 spin profile에 맞아야 합니다.', 'The guaranteed final piece must match the inventory and spin profile.'],
      worker_count_invalid: ['Worker 수는 1 이상이어야 합니다.', 'Worker count must be positive.']
    };
    return messages[code][korean ? 0 : 1];
  }
</script>

<svelte:head>
  <title>{korean ? 'Spin 구조' : 'Spin structure'} · Clearra</title>
  <meta name="description" content="Exact unordered no-hold spin structure search, cover, and guarantee" />
</svelte:head>

<WorkspaceShell
  activeMode="spin-structure"
  {language}
  {active}
  statusLabel={label(runtimeView.status)}
  workspaceLabel={korean ? 'Spin 구조' : 'Spin structure'}
  dimensionLabel={korean ? '필드 높이' : 'Field height'}
  dimensionValue={request.visibleHeight}
  dimensionMin={4}
  dimensionMax={24}
  cancelLabel={label('cancel')}
  runLabel={label('run')}
  runDisabled={validationCodes.length > 0}
  on:language={(event) => setLanguage(event.detail)}
  on:dimension={(event) => setHeight(event.detail)}
  on:cancel={cancel}
  on:run={run}
>
  <section slot="editor" class="editor" aria-label={korean ? 'Spin 구조 입력' : 'Spin structure input'}>
    <label>
      <span>{korean ? 'Board mask v1' : 'Board mask v1'}</span>
      <textarea rows="5" spellcheck="false" value={request.boardMaskV1} on:input={(event) => updateRequest({ boardMaskV1: (event.currentTarget as HTMLTextAreaElement).value })}></textarea>
      <small>{korean ? 'CTK3 canonical 24-row occupied mask: 소문자 16진수 60자리' : 'CTK3 canonical 24-row occupied mask: 60 lowercase hexadecimal digits'}</small>
    </label>
    <label>
      <span>{korean ? '순서 없는 no-hold inventory' : 'Unordered no-hold inventory'}</span>
      <input value={request.inventory} spellcheck="false" placeholder="IOTSZJL" on:input={(event) => setInventory((event.currentTarget as HTMLInputElement).value)} />
    </label>
    <p class="authority">{korean ? 'Queue/pattern과 Hold는 이 기능의 입력이 아닙니다. inventory의 중복 문자는 수량으로 보존됩니다.' : 'Queue/pattern and hold are not inputs to this capability. Repeated inventory letters preserve multiplicity.'}</p>
  </section>

  <section slot="controls" class="controls" aria-label={korean ? 'Spin 구조 제어' : 'Spin structure controls'}>
    <label><span>{korean ? '기능' : 'Capability'}</span><select value={request.mode} on:change={(event) => updateRequest({ mode: (event.currentTarget as HTMLSelectElement).value as SpinStructureRequest['mode'] })}><option value="search">search</option><option value="cover">cover</option><option value="guaranteed">guaranteed</option></select></label>
    <div class="two-columns">
      <label><span>{korean ? 'Spin 프로필' : 'Spin profile'}</span><select value={request.spinProfile} on:change={(event) => updateRequest({ spinProfile: (event.currentTarget as HTMLSelectElement).value as SpinStructureRequest['spinProfile'] })}><option value="t-spins">t-spins</option><option value="t-spins-plus">t-spins-plus</option><option value="all-mini">all-mini</option><option value="all-mini-plus">all-mini-plus</option><option value="all-spin">all-spin</option><option value="all-spin-plus">all-spin-plus</option></select></label>
      <label><span>{korean ? 'Line 조건' : 'Line requirement'}</span><select value={request.lines} on:change={(event) => updateRequest({ lines: (event.currentTarget as HTMLSelectElement).value as SpinStructureRequest['lines'] })}>{#each ['any', '0', '1', '2', '3', '4', '1+', '2+', '3+', '4+'] as lines}<option value={lines}>{lines}</option>{/each}</select></label>
      <label><span>{korean ? 'Fill 시작' : 'Fill bottom'}</span><input type="number" min="0" max={request.visibleHeight - 1} value={request.fillBottom} on:input={(event) => updateRequest({ fillBottom: Number((event.currentTarget as HTMLInputElement).value) })} /></label>
      <label><span>{korean ? 'Fill 끝(exclusive)' : 'Fill top (exclusive)'}</span><input type="number" min="1" max={request.visibleHeight} value={request.fillTop} on:input={(event) => updateRequest({ fillTop: Number((event.currentTarget as HTMLInputElement).value) })} /></label>
      <label><span>{korean ? '규칙' : 'Rule'}</span><select value={request.rule} on:change={(event) => updateRequest({ rule: (event.currentTarget as HTMLSelectElement).value as SpinStructureRequest['rule'] })}><option value="srs-plus">srs-plus</option><option value="srs">srs</option><option value="srs-x">srs-x</option><option value="jstris-180">jstris-180</option><option value="no-kick">no-kick</option></select></label>
      <label><span>{korean ? '최대 배치' : 'Maximum placements'}</span><input type="number" min="1" max={Math.max(1, normalizedSpinInventory(request.inventory).length)} value={request.maxPlacements} on:input={(event) => updateRequest({ maxPlacements: Number((event.currentTarget as HTMLInputElement).value) })} /></label>
      <label><span>{korean ? '최소성' : 'Minimality'}</span><select value={request.minimality} on:change={(event) => updateRequest({ minimality: (event.currentTarget as HTMLSelectElement).value as SpinStructureRequest['minimality'] })}><option value="subset-minimal">subset-minimal</option><option value="minimum-piece-count">minimum-piece-count</option></select></label>
      <label><span>{korean ? 'Worker 수' : 'Workers'}</span><input type="number" min="1" value={request.workers} disabled={request.useAllLogicalProcessors} on:input={(event) => updateRequest({ workers: Number((event.currentTarget as HTMLInputElement).value) })} /></label>
    </div>
    {#if request.mode !== 'search'}
      <section class="route-options">
        <label><span>{korean ? '최대 inventory 순열' : 'Maximum inventory patterns'}</span><input type="number" min="1" max="100000" value={request.maxPatterns} on:input={(event) => updateRequest({ maxPatterns: Number((event.currentTarget as HTMLInputElement).value) })} /></label>
        {#if request.mode === 'cover'}
          <p>{korean ? 'Objective는 min-cover로 고정됩니다. 모든 동일 최소 크기 exact portfolio를 공통 결과 pager로 확인합니다.' : 'The objective is fixed to min-cover. The common result pager exposes every exact equal-cardinality optimum.'}</p>
        {:else}
          <label><span>{korean ? '마지막 조각' : 'Final piece'}</span><select value={request.finalPiece} on:change={(event) => updateRequest({ finalPiece: (event.currentTarget as HTMLSelectElement).value as SpinStructureRequest['finalPiece'] })}>{#each ['I', 'O', 'T', 'S', 'Z', 'J', 'L'] as piece}<option value={piece}>{piece}</option>{/each}</select></label>
          <label class="check-row"><input type="checkbox" checked={request.dependencyReport} on:change={(event) => updateRequest({ dependencyReport: (event.currentTarget as HTMLInputElement).checked })} /><span>{korean ? '의존성 보고서 포함' : 'Include dependency report'}</span></label>
          <p>{korean ? 'Guaranteed는 모든 고유 비-target 순서를 exact replay하고 마지막 조각을 고정하는 일반 완전 family입니다.' : 'Guaranteed exact-replays every unique non-target order with the final piece fixed; it remains an ordinary complete family.'}</p>
        {/if}
      </section>
    {/if}
    <label class="check-row"><input type="checkbox" checked={request.useAllLogicalProcessors} on:change={(event) => updateRequest({ useAllLogicalProcessors: (event.currentTarget as HTMLInputElement).checked })} /><span>{korean ? '모든 논리 프로세서' : 'All logical processors'}</span></label>
    <p class="authority">{korean ? 'Spin 구조는 CPU 전용이며 backend fallback, GPU, tablebase, memory 옵션을 노출하지 않습니다.' : 'Spin structure is CPU-only and exposes no backend fallback, GPU, tablebase, or memory option.'}</p>
    {#if validationCodes.length}
      <ul class="errors" aria-live="polite">{#each validationCodes as code}<li>{errorLabel(code)}</li>{/each}</ul>
    {/if}
  </section>

  <ProductFamilyResult
    slot="result"
    view={runtimeView}
    {language}
    {elapsedMs}
    capabilityLabel={korean ? 'Spin 구조' : 'Spin structure'}
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

<style>
  .editor, .controls { display: grid; gap: 14px; }
  label { display: grid; gap: 6px; min-width: 0; }
  label > span { color: #53605b; font-size: 11px; font-weight: 720; }
  input, select, textarea { background: #fff; border: 1px solid #cbd3ce; border-radius: 5px; color: #26322e; font-size: 12px; min-width: 0; padding: 0 10px; width: 100%; }
  input, select { height: 39px; }
  textarea { font-family: ui-monospace, SFMono-Regular, Consolas, monospace; line-height: 1.5; overflow-wrap: anywhere; padding-bottom: 9px; padding-top: 9px; resize: vertical; }
  input:focus, select:focus, textarea:focus { border-color: #16877d; box-shadow: 0 0 0 3px #16877d1f; outline: 0; }
  small, .route-options p { color: #68736f; font-size: 10px; line-height: 1.5; }
  .two-columns { display: grid; gap: 10px; grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .check-row { align-items: center; display: flex; gap: 8px; min-height: 39px; }
  .check-row input { height: 16px; margin: 0; width: 16px; }
  .route-options { background: #f5f8f6; border: 1px solid #dce3df; border-radius: 6px; display: grid; gap: 10px; padding: 12px; }
  .route-options p { margin: 0; }
  .authority { background: #f7f3ea; border: 1px solid #e3d8bd; border-radius: 5px; color: #725d29; font-size: 10px; line-height: 1.5; margin: 0; padding: 9px 10px; }
  .errors { background: #fff1f0; border: 1px solid #efc3be; border-radius: 5px; color: #8b2820; display: grid; font-size: 11px; gap: 4px; margin: 0; padding: 9px 12px 9px 28px; }
  @media (max-width: 560px) { .two-columns { grid-template-columns: 1fr; } }
</style>
