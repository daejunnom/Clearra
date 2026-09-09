<script lang="ts">
  import { componentMessage } from '../i18n/componentCatalog';
  import { readWorkspaceLanguage, persistWorkspaceLanguage } from './workspaceLanguagePreference';
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
  import WorkspaceFailureNotice from './WorkspaceFailureNotice.svelte';
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
    workspaceMessage,
    type WorkspaceLanguage
  } from './workspaceI18n';
  import { workspaceViewFromDesktop, workspaceViewFromWasm } from './workspaceRuntime';
  import { workspacePublicFailure } from './workspacePublicFailure';

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
  $: publicResultFailures = resultError
    ? [workspacePublicFailure('result-invalid')]
    : runtimeView.publicFailures;
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
    language = readWorkspaceLanguage();
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
    persistWorkspaceLanguage(next);
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
      actionMessage = componentMessage(language, 'documentCopied');
    } catch {
      actionMessage = componentMessage(language, 'documentCopyFailed');
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
          ? (componentMessage(language, 'fumenDocumentsOnePerLine'))
          : (componentMessage(language, 'typedFieldDocument'))}</span>
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
          <span>{componentMessage(language, 'transform')}</span>
          <select bind:value={transform} disabled={active}>
            {#each transforms as value}<option value={value}>{value}</option>{/each}
          </select>
        </label>
        {#if transform === 'get-page'}
          <label><span>{componentMessage(language, 'page1Based')}</span><input type="number" min="1" step="1" bind:value={pageNumber} disabled={active} /></label>
        {:else if transform === 'page-shift'}
          <label><span>{componentMessage(language, 'leftPageShift')}</span><input type="number" step="1" bind:value={pageShift} disabled={active} /></label>
        {/if}
      </div>
      {#if transform === 'text-to-fumen'}
        <label>
          <span>{componentMessage(language, 'pageCommentsOnePerLine')}</span>
          <textarea rows="7" bind:value={comments} disabled={active}></textarea>
        </label>
      {/if}
    {:else if tool === 'render'}
      <div class="option-grid">
        <label><span>{componentMessage(language, 'format')}</span><select bind:value={artifactFormat} disabled={active}><option value="png">PNG</option><option value="gif">GIF</option></select></label>
        {#if artifactFormat === 'png'}
          <label><span>{componentMessage(language, 'page1Based')}</span><input type="number" min="1" step="1" bind:value={pageNumber} disabled={active} /></label>
        {/if}
      </div>
    {:else if tool === 'parity'}
      <small>{componentMessage(language, 'thisReportIsStaticObservationOnlyIt')}</small>
    {:else if tool === 'to-gray'}
      <small>{componentMessage(language, 'onlyOccupiedColorsBecomeGrayPageOperation')}</small>
    {:else}
      <small>{componentMessage(language, 'fieldGarbageAndOperationPieceRotationAre')}</small>
    {/if}
  </div>

  <section slot="result" class="result" aria-live="polite">
    {#if activeParityPage}
      <header class="result-header">
        <h2>{label('utilityParity')}</h2>
        <nav>
          <button type="button" disabled={parityPageIndex === 0} on:click={() => (parityPageIndex -= 1)} aria-label={componentMessage(language, 'surfacePreviousPage')}><ChevronLeft size={16} /></button>
          <span>{activeParityPage.page_number} / {activeParityPage.total_pages}</span>
          <button type="button" disabled={pageLoading || (parityExhausted && parityPageIndex + 1 >= parityPages.length)} on:click={nextParityPage} aria-label={componentMessage(language, 'surfaceNextPage')}><ChevronRight size={16} /></button>
        </nav>
      </header>
      <dl>
        <div><dt>{componentMessage(language, 'surfaceCoordinateBasis')}</dt><dd>{activeParityPage.coordinate_basis}</dd></div>
        <div><dt>{componentMessage(language, 'surfaceDimensions')}</dt><dd>{activeParityPage.width} × {activeParityPage.height}</dd></div>
        <div><dt>{componentMessage(language, 'surfaceOccupiedCellCount')}</dt><dd>{activeParityPage.occupied_cell_count}</dd></div>
        <div><dt>{componentMessage(language, 'surfaceCheckerBlackWhiteDelta')}</dt><dd>{activeParityPage.checker_black_count} / {activeParityPage.checker_white_count} / {activeParityPage.checker_delta}</dd></div>
        <div><dt>{componentMessage(language, 'surfaceFourColorCounts')}</dt><dd>{activeParityPage.four_color_counts.join(', ')}</dd></div>
        <div><dt>{componentMessage(language, 'surfaceColumnEvenOddDelta')}</dt><dd>{activeParityPage.even_column_count} / {activeParityPage.odd_column_count} / {activeParityPage.column_parity_delta}</dd></div>
        <div><dt>{componentMessage(language, 'surfaceOccupiedAreaModFour')}</dt><dd>{activeParityPage.occupied_area_mod_four}</dd></div>
        <div><dt>{componentMessage(language, 'surfacePendingGarbageOccupiedCellCount')}</dt><dd>{activeParityPage.pending_garbage_occupied_cell_count}</dd></div>
        <div><dt>{componentMessage(language, 'surfaceFeasibilityClaim')}</dt><dd>false</dd></div>
        <div><dt>{componentMessage(language, 'surfacePruningAuthority')}</dt><dd>none</dd></div>
      </dl>
    {:else if fieldDocuments.length > 0}
      <h2>{label(titleKey)}</h2>
      <ol class="documents">
        {#each fieldDocuments as output, index (output.canonical_sha256)}
          <li>
            <div><strong>{output.filename}</strong><span>{output.page_count} {componentMessage(language, 'surfacePageS')} {output.canonical_sha256}</span></div>
            <code>{output.document}</code>
            <div class="actions">
              <button type="button" on:click={() => copyDocument(output)}><Copy size={15} />{componentMessage(language, 'copy')}</button>
              <button type="button" on:click={() => downloadDocument(output)}><Download size={15} />{componentMessage(language, 'download')}</button>
            </div>
          </li>
        {/each}
      </ol>
    {:else if renderArtifact && artifactUrl}
      <h2>{label('utilityRender')}</h2>
      <figure>
        <img src={artifactUrl} alt={componentMessage(language, 'exactFieldRenderResult')} />
        <figcaption>{renderArtifact.filename} · {renderArtifact.byte_length} {componentMessage(language, 'surfaceBytesSha256')} {renderArtifact.sha256}</figcaption>
      </figure>
      <button class="download" type="button" on:click={downloadArtifact}><Download size={16} />{componentMessage(language, 'downloadArtifact')}</button>
    {:else if publicResultFailures.length}
      <WorkspaceFailureNotice failures={publicResultFailures} {language} compact />
    {:else}
      <p class="empty">{componentMessage(language, 'runTheUtilityToDisplayItsTyped')}</p>
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
  dl, .documents, figure, .empty { background: #fff; border: 1px solid #d5dcd7; border-radius: 7px; margin: 0; padding: 10px 18px; }
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
  .action-message { color: #075f58; font-size: 12px; }
  @media (max-width: 720px) {
    .option-grid, dl div { grid-template-columns: 1fr; }
    .result { padding-left: 16px; padding-right: 16px; }
  }
</style>
