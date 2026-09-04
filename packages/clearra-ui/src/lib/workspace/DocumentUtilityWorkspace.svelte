<script lang="ts">
  import { ChevronLeft, ChevronRight, Copy, Download } from '@lucide/svelte';
  import { getContext, onDestroy, onMount } from 'svelte';
  import { get } from 'svelte/store';

  import {
    loadNextProductPage as loadNextDesktopProductPage,
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
    clearWasmTerminalResult,
    sharedBrowserHostCapabilitySnapshot,
    updateWasmCommandText,
    wasmWorkerState,
    WasmTerminalWorkerController,
    type ClearraFieldDocumentPayload,
    type ClearraParityReportPagePayload,
    type ClearraProductResultPayload,
    type HostCapabilitySnapshot
  } from '../wasm';
  import WorkspaceShell from './WorkspaceShell.svelte';
  import {
    buildDocumentUtilityCommand,
    decodeValidatedRenderArtifact,
    detectFieldDocumentFormat,
    documentUtilityRequestForDesktop,
    fumenDocumentInputs,
    isBoundedCanonicalFieldDocument,
    validateFieldDocumentPayload,
    type DocumentUtilityCommandInput,
    type DocumentUtilityFumenTransform,
    type DocumentUtilityTool
  } from './documentUtilityModel';
  import { validateProductResultPayload } from './productResultPager';
  import {
    preferredWorkspaceLanguage,
    workspaceMessage,
    type WorkspaceLanguage
  } from './workspaceI18n';
  import { workspaceViewFromDesktop, workspaceViewFromWasm } from './workspaceRuntime';

  export let tool: DocumentUtilityTool;
  export let workerFactory: (() => Worker) | null = null;
  export let runtime: 'web' | 'desktop' = 'web';

  const transforms: DocumentUtilityFumenTransform[] = [
    'roundtrip',
    'combine',
    'split',
    'get-page',
    'page-shift',
    'clean-comments',
    'preserve-comments',
    'to-gray',
    'mirror',
    'text-to-fumen'
  ];
  const hostCapabilitySnapshot =
    getContext<HostCapabilitySnapshot>(HOST_CAPABILITY_SNAPSHOT_CONTEXT) ??
    sharedBrowserHostCapabilitySnapshot();
  const workerController = new WasmTerminalWorkerController(workerFactory, hostCapabilitySnapshot);

  let language: WorkspaceLanguage = 'en';
  let document = '';
  let transform: DocumentUtilityFumenTransform = 'roundtrip';
  let pageNumber = 1;
  let pageShift = 0;
  let comments = '';
  let artifactFormat: 'png' | 'gif' = 'png';
  let disposed = false;
  let acceptedPayload: ClearraProductResultPayload | null = null;
  let observedPayload: ClearraProductResultPayload | null = null;
  let resultError = '';
  let actionMessage = '';
  let parityPages: ClearraParityReportPagePayload[] = [];
  let parityPageIndex = 0;
  let parityExhausted = false;
  let pageHandleOwned = false;
  let pageLoading = false;
  let artifactUrl = '';
  let artifactBlob: Blob | null = null;
  let artifactGeneration = 0;

  $: workerController.setWorkerFactory(workerFactory);
  $: runtimeView = runtime === 'web'
    ? workspaceViewFromWasm($wasmWorkerState)
    : workspaceViewFromDesktop($desktopJobState);
  $: active = runtimeView.status === 'running' || runtimeView.status === 'cancelling';
  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);
  $: productPayload = runtimeView.response?.product_result_payload ?? null;
  $: if (productPayload !== observedPayload) acceptProductPayload(productPayload);
  $: normalizedDocument = document.trim();
  $: detectedFormat = detectFieldDocumentFormat(normalizedDocument);
  $: fumenDocuments = fumenDocumentInputs(document, transform === 'combine');
  $: commentValues = comments.split(/\r?\n/u).map((value) => value.trim()).filter(Boolean);
  $: validInput = tool === 'fumen'
    ? validFumenInput()
    : isBoundedCanonicalFieldDocument(normalizedDocument) &&
      (tool !== 'render' || artifactFormat === 'gif' || (Number.isInteger(pageNumber) && pageNumber >= 1));
  $: activeParityPage = parityPages[parityPageIndex] ?? null;
  $: fieldDocuments = acceptedPayload?.content.payload_kind === 'field-document'
    ? [acceptedPayload.content.payload]
    : acceptedPayload?.content.payload_kind === 'field-document-set'
      ? acceptedPayload.content.payload.documents
      : [];
  $: renderArtifact = acceptedPayload?.content.payload_kind === 'render-artifact'
    ? acceptedPayload.content.payload
    : null;
  $: titleKey = tool === 'parity'
    ? 'utilityParity'
    : tool === 'fumen'
      ? 'utilityFumen'
      : tool === 'render'
        ? 'utilityRender'
        : tool === 'to-gray'
          ? 'utilityToGray'
          : 'utilityMirror';

  onMount(() => {
    language = preferredWorkspaceLanguage(
      localStorage.getItem('clearra-language') ?? navigator.language
    );
    if (runtime === 'web') {
      clearWasmTerminalResult();
      workerController.prewarm(1, false, CPU_ONLY_RUNTIME_WARMUP_POLICY);
    } else {
      clearDesktopTerminalResult();
      resumeDesktopJobPolling();
    }
  });

  onDestroy(disposeWorkspace);

  function validFumenInput(): boolean {
    if (transform === 'text-to-fumen') return commentValues.length > 0;
    if (transform === 'combine') return fumenDocuments.length > 0;
    if (fumenDocuments.length !== 1) return false;
    if (transform === 'get-page') return Number.isInteger(pageNumber) && pageNumber >= 1;
    if (transform === 'page-shift') return Number.isSafeInteger(pageShift);
    return true;
  }

  function setLanguage(next: WorkspaceLanguage) {
    language = next;
    localStorage.setItem('clearra-language', next);
  }

  async function run() {
    if (active || !validInput) return;
    await releasePageOwner();
    clearAcceptedResult();
    const commandInput = documentUtilityCommandInput();
    if (runtime === 'web') {
      updateWasmCommandText(buildDocumentUtilityCommand(commandInput));
      workerController.run();
      return;
    }
    updateDesktopRequest(documentUtilityRequestForDesktop(commandInput, language));
    await startDesktopJob();
  }

  function documentUtilityCommandInput(): DocumentUtilityCommandInput {
    return {
      tool,
      format: detectedFormat ?? 'ctk3',
      document: normalizedDocument,
      transform,
      documents: fumenDocuments,
      pageNumber,
      pageShift,
      comments: commentValues,
      artifactFormat
    };
  }

  async function cancel() {
    if (!active) return;
    if (runtime === 'web') workerController.cancel();
    else await cancelDesktopJob();
  }

  function acceptProductPayload(payload: ClearraProductResultPayload | null) {
    observedPayload = payload;
    acceptedPayload = null;
    resultError = '';
    actionMessage = '';
    parityPages = [];
    parityPageIndex = 0;
    parityExhausted = false;
    pageHandleOwned = false;
    revokeArtifact();
    if (!payload) return;
    const error = validateProductResultPayload(payload);
    if (error) {
      resultError = error;
      return;
    }
    const payloadKind = payload.content.payload_kind;
    const resultMatches = tool === 'parity'
      ? payloadKind === 'parity-report-page'
      : tool === 'render'
        ? payloadKind === 'render-artifact'
        : tool === 'fumen'
          ? ['field-document', 'field-document-set'].includes(payloadKind)
          : payloadKind === 'field-document';
    if (!resultMatches) {
      resultError = 'typed result does not match the selected utility';
      return;
    }
    acceptedPayload = payload;
    if (payload.content.payload_kind === 'parity-report-page') {
      parityPages = [payload.content.payload];
      pageHandleOwned = payload.content.payload.page_handle_available;
      parityExhausted = payload.content.payload.total_pages === 1;
      if (parityExhausted) void releasePageOwner();
    } else if (payload.content.payload_kind === 'render-artifact') {
      void prepareArtifact(payload.content.payload, ++artifactGeneration);
    }
  }

  async function prepareArtifact(
    payload: Extract<ClearraProductResultPayload, { content: { payload_kind: 'render-artifact' } }>['content']['payload'],
    generation: number
  ) {
    try {
      const bytes = await decodeValidatedRenderArtifact(payload);
      if (generation !== artifactGeneration || disposed) return;
      artifactBlob = new Blob([bytes], { type: payload.media_type });
      artifactUrl = URL.createObjectURL(artifactBlob);
    } catch (reason) {
      if (generation === artifactGeneration) {
        resultError = reason instanceof Error ? reason.message : String(reason);
        acceptedPayload = null;
      }
    }
  }

  async function nextParityPage() {
    if (pageLoading || parityExhausted) return;
    if (parityPageIndex + 1 < parityPages.length) {
      parityPageIndex += 1;
      return;
    }
    pageLoading = true;
    resultError = '';
    try {
      const response = runtime === 'web'
        ? await workerController.loadNextProductPage(undefined, 1)
        : await loadNextDesktopProductPage(1);
      if (response.product_page_kind !== 'parity-report') {
        throw new Error('product page kind does not match the parity report');
      }
      if (response.state === 'exhausted') {
        parityExhausted = true;
        await releasePageOwner();
        return;
      }
      const page = response.page;
      const first = parityPages[0];
      if (
        page.page_number !== parityPages.length + 1 ||
        page.total_pages !== first?.total_pages ||
        page.document_format !== first?.document_format ||
        page.feasibility_claim !== false ||
        page.pruning_authority !== 'none'
      ) {
        throw new Error('parity page does not match the retained report');
      }
      parityPages = [...parityPages, page];
      parityPageIndex += 1;
      if (page.page_number === page.total_pages) {
        parityExhausted = true;
        await releasePageOwner();
      }
    } catch (reason) {
      resultError = reason instanceof Error ? reason.message : String(reason);
    } finally {
      pageLoading = false;
    }
  }

  async function copyDocument(payload: ClearraFieldDocumentPayload) {
    if (validateFieldDocumentPayload(payload)) return;
    try {
      await navigator.clipboard.writeText(payload.document);
      actionMessage = language === 'ko' ? '문서를 복사했습니다.' : 'Document copied.';
    } catch {
      actionMessage = language === 'ko' ? '문서를 복사하지 못했습니다.' : 'Document copy failed.';
    }
  }

  function downloadDocument(payload: ClearraFieldDocumentPayload) {
    if (validateFieldDocumentPayload(payload)) return;
    downloadBlob(new Blob([payload.document], { type: 'text/plain;charset=utf-8' }), payload.filename);
  }

  function downloadArtifact() {
    if (!artifactBlob || !renderArtifact) return;
    downloadBlob(artifactBlob, renderArtifact.filename);
  }

  function downloadBlob(blob: Blob, filename: string) {
    const url = URL.createObjectURL(blob);
    const anchor = globalThis.document.createElement('a');
    anchor.href = url;
    anchor.download = filename;
    anchor.rel = 'noopener';
    anchor.click();
    setTimeout(() => URL.revokeObjectURL(url), 0);
  }

  async function releasePageOwner() {
    if (!pageHandleOwned) return;
    pageHandleOwned = false;
    try {
      if (runtime === 'web') workerController.releaseProductPages();
      else await releaseDesktopProductPages();
    } catch {
      resultError ||= 'product page owner release failed';
    }
  }

  function clearAcceptedResult() {
    acceptedPayload = null;
    observedPayload = null;
    resultError = '';
    actionMessage = '';
    parityPages = [];
    parityPageIndex = 0;
    parityExhausted = false;
    revokeArtifact();
  }

  function revokeArtifact() {
    artifactGeneration += 1;
    if (artifactUrl) URL.revokeObjectURL(artifactUrl);
    artifactUrl = '';
    artifactBlob = null;
  }

  function disposeWorkspace() {
    if (disposed) return;
    disposed = true;
    void releasePageOwner();
    revokeArtifact();
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
</script>

<svelte:head>
  <title>{label(titleKey)} · Clearra</title>
</svelte:head>

<WorkspaceShell
  activeMode={tool}
  {language}
  {active}
  statusLabel={label(runtimeView.status)}
  workspaceLabel={label(titleKey)}
  dimensionLabel=""
  dimensionValue={1}
  showDimension={false}
  cancelLabel={label('cancel')}
  runLabel={label('run')}
  runDisabled={!validInput}
  singlePanel
  on:language={(event) => setLanguage(event.detail)}
  on:cancel={cancel}
  on:run={run}
>
  <div slot="controls" class="controls">
    {#if tool !== 'fumen' || transform !== 'text-to-fumen'}
      <label>
        <span>{tool === 'fumen' && transform === 'combine'
          ? (language === 'ko' ? 'Fumen 문서 (한 줄에 하나)' : 'Fumen documents (one per line)')
          : (language === 'ko' ? 'Typed field 문서' : 'Typed field document')}</span>
        <textarea
          rows="7"
          bind:value={document}
          disabled={active}
          placeholder={tool === 'fumen' ? 'v115@…' : 'ctk3_… or v115@…'}
          aria-invalid={document.length > 0 && !validInput}
        ></textarea>
      </label>
    {/if}
    {#if tool === 'fumen'}
      <div class="option-grid">
        <label>
          <span>{language === 'ko' ? '변환' : 'Transform'}</span>
          <select bind:value={transform} disabled={active}>
            {#each transforms as value}<option value={value}>{value}</option>{/each}
          </select>
        </label>
        {#if transform === 'get-page'}
          <label><span>{language === 'ko' ? '페이지 (1부터)' : 'Page (1-based)'}</span><input type="number" min="1" step="1" bind:value={pageNumber} disabled={active} /></label>
        {:else if transform === 'page-shift'}
          <label><span>{language === 'ko' ? '왼쪽 이동량' : 'Left page shift'}</span><input type="number" step="1" bind:value={pageShift} disabled={active} /></label>
        {/if}
      </div>
      {#if transform === 'text-to-fumen'}
        <label>
          <span>{language === 'ko' ? '페이지 주석 (한 줄에 하나)' : 'Page comments (one per line)'}</span>
          <textarea rows="7" bind:value={comments} disabled={active}></textarea>
        </label>
      {/if}
    {:else if tool === 'render'}
      <div class="option-grid">
        <label><span>{language === 'ko' ? '형식' : 'Format'}</span><select bind:value={artifactFormat} disabled={active}><option value="png">PNG</option><option value="gif">GIF</option></select></label>
        {#if artifactFormat === 'png'}
          <label><span>{language === 'ko' ? '페이지 (1부터)' : 'Page (1-based)'}</span><input type="number" min="1" step="1" bind:value={pageNumber} disabled={active} /></label>
        {/if}
      </div>
    {:else if tool === 'parity'}
      <small>{language === 'ko'
        ? '이 보고서는 정적 관찰만 제공하며 가능성 판정이나 가지치기 권위를 주장하지 않습니다. pending garbage는 별도 집계됩니다.'
        : 'This report is static observation only. It claims neither feasibility nor pruning authority, and preserves pending garbage separately.'}</small>
    {:else if tool === 'to-gray'}
      <small>{language === 'ko'
        ? '점유 색상만 회색으로 바꾸며 페이지, operation, 주석, garbage, 크기 identity는 보존합니다.'
        : 'Only occupied colors become gray; page, operation, comment, garbage, and dimension identity are preserved.'}</small>
    {:else}
      <small>{language === 'ko'
        ? '필드, garbage, operation의 조각·회전을 함께 좌우 반전합니다. 같은 문서를 두 번 반전하면 원 identity로 돌아옵니다.'
        : 'Field, garbage, and operation piece/rotation are mirrored together; mirroring twice restores the original identity.'}</small>
    {/if}
  </div>

  <section slot="result" class="result" aria-live="polite">
    {#if activeParityPage}
      <header class="result-header">
        <h2>{label('utilityParity')}</h2>
        <nav>
          <button type="button" disabled={parityPageIndex === 0} on:click={() => (parityPageIndex -= 1)} aria-label="Previous page"><ChevronLeft size={16} /></button>
          <span>{activeParityPage.page_number} / {activeParityPage.total_pages}</span>
          <button type="button" disabled={pageLoading || (parityExhausted && parityPageIndex + 1 >= parityPages.length)} on:click={nextParityPage} aria-label="Next page"><ChevronRight size={16} /></button>
        </nav>
      </header>
      <dl>
        <div><dt>coordinate_basis</dt><dd>{activeParityPage.coordinate_basis}</dd></div>
        <div><dt>dimensions</dt><dd>{activeParityPage.width} × {activeParityPage.height}</dd></div>
        <div><dt>occupied_cell_count</dt><dd>{activeParityPage.occupied_cell_count}</dd></div>
        <div><dt>checker_black / white / delta</dt><dd>{activeParityPage.checker_black_count} / {activeParityPage.checker_white_count} / {activeParityPage.checker_delta}</dd></div>
        <div><dt>four_color_counts</dt><dd>{activeParityPage.four_color_counts.join(', ')}</dd></div>
        <div><dt>column even / odd / delta</dt><dd>{activeParityPage.even_column_count} / {activeParityPage.odd_column_count} / {activeParityPage.column_parity_delta}</dd></div>
        <div><dt>occupied_area_mod_four</dt><dd>{activeParityPage.occupied_area_mod_four}</dd></div>
        <div><dt>pending_garbage_occupied_cell_count</dt><dd>{activeParityPage.pending_garbage_occupied_cell_count}</dd></div>
        <div><dt>feasibility_claim</dt><dd>false</dd></div>
        <div><dt>pruning_authority</dt><dd>none</dd></div>
      </dl>
    {:else if fieldDocuments.length > 0}
      <h2>{label(titleKey)}</h2>
      <ol class="documents">
        {#each fieldDocuments as output, index (output.canonical_sha256)}
          <li>
            <div><strong>{output.filename}</strong><span>{output.page_count} page(s) · {output.canonical_sha256}</span></div>
            <code>{output.document}</code>
            <div class="actions">
              <button type="button" on:click={() => copyDocument(output)}><Copy size={15} />{language === 'ko' ? '복사' : 'Copy'}</button>
              <button type="button" on:click={() => downloadDocument(output)}><Download size={15} />{language === 'ko' ? '다운로드' : 'Download'}</button>
            </div>
          </li>
        {/each}
      </ol>
    {:else if renderArtifact && artifactUrl}
      <h2>{label('utilityRender')}</h2>
      <figure>
        <img src={artifactUrl} alt={language === 'ko' ? '정확한 필드 렌더 결과' : 'Exact field render result'} />
        <figcaption>{renderArtifact.filename} · {renderArtifact.byte_length} bytes · SHA-256 {renderArtifact.sha256}</figcaption>
      </figure>
      <button class="download" type="button" on:click={downloadArtifact}><Download size={16} />{language === 'ko' ? '아티팩트 다운로드' : 'Download artifact'}</button>
    {:else if resultError || runtimeView.error}
      <p class="error" role="alert">{resultError || runtimeView.error}</p>
    {:else}
      <p class="empty">{language === 'ko' ? '실행하면 typed 결과가 여기에 표시됩니다.' : 'Run the utility to display its typed result.'}</p>
    {/if}
    {#if actionMessage}<p class="action-message">{actionMessage}</p>{/if}
  </section>
</WorkspaceShell>

<style>
  .controls, label { display: grid; gap: 8px; }
  .controls { gap: 18px; }
  label > span { color: #4c5954; font-size: 12px; font-weight: 750; }
  textarea, select, input { background: #fff; border: 1px solid #cbd3ce; border-radius: 5px; color: #26322e; font: inherit; padding: 10px; }
  textarea { min-height: 140px; resize: vertical; word-break: break-all; }
  textarea[aria-invalid='true'] { border-color: #b84a4a; }
  small { color: #65716c; line-height: 1.5; }
  .option-grid { display: grid; gap: 12px; grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .result { margin: 0 auto; max-width: 1460px; padding: 8px 24px 40px; }
  .result h2 { font-size: 17px; margin: 0 0 14px; }
  .result-header, .result-header nav, .actions, .download { align-items: center; display: flex; }
  .result-header { justify-content: space-between; }
  .result-header nav { gap: 8px; }
  button { align-items: center; background: #fff; border: 1px solid #cbd3ce; border-radius: 5px; color: #26322e; cursor: pointer; display: inline-flex; gap: 6px; min-height: 34px; padding: 6px 10px; }
  button:disabled { cursor: not-allowed; opacity: .45; }
  dl, .documents, figure, .empty, .error { background: #fff; border: 1px solid #d5dcd7; border-radius: 7px; margin: 0; padding: 10px 18px; }
  dl div { display: grid; gap: 16px; grid-template-columns: minmax(240px, .42fr) minmax(0, 1fr); padding: 9px 0; }
  dl div + div { border-top: 1px solid #e4e9e6; }
  dt { color: #596560; font-size: 12px; font-weight: 750; }
  dd, code { font-family: ui-monospace, SFMono-Regular, Consolas, monospace; overflow-wrap: anywhere; }
  dd { margin: 0; }
  .documents { list-style: none; padding: 0 18px; }
  .documents li { display: grid; gap: 10px; padding: 15px 0; }
  .documents li + li { border-top: 1px solid #e4e9e6; }
  .documents li > div:first-child { display: flex; flex-wrap: wrap; gap: 8px 16px; justify-content: space-between; }
  .documents span, figcaption { color: #65716c; font-size: 11px; overflow-wrap: anywhere; }
  .actions { gap: 8px; }
  figure { display: grid; gap: 12px; justify-items: center; }
  figure img { image-rendering: pixelated; max-height: 620px; max-width: 100%; }
  .download { margin-top: 12px; }
  .error { color: #9b3030; }
  .action-message { color: #075f58; font-size: 12px; }
  @media (max-width: 720px) {
    .option-grid, dl div { grid-template-columns: 1fr; }
    .result { padding-left: 16px; padding-right: 16px; }
  }
</style>
