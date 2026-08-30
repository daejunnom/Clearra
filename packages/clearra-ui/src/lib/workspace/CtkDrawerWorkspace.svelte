<!-- SRP rationale: this component has one change reason: the complete CTK document-editing workspace interaction contract. -->
<script lang="ts">
  import {
    AlertTriangle,
    Check,
    ChevronLeft,
    ChevronRight,
    ClipboardCopy,
    Copy,
    Download,
    FileUp,
    HelpCircle,
    Image,
    LoaderCircle,
    Plus,
    Trash2,
    Upload
  } from '@lucide/svelte';
  import { getContext, onDestroy, onMount, tick } from 'svelte';

  import {
    HOST_CAPABILITY_SNAPSHOT_CONTEXT,
    automaticWorkerAuthority,
    clearWasmTerminalResult,
    sharedBrowserHostCapabilitySnapshot,
    updateWasmCommandText,
    wasmWorkerState,
    WasmTerminalWorkerController,
    type ClearraProductResultPayload,
    type HostCapabilitySnapshot
  } from '../wasm';
  import { writeClipboardText } from './clipboardText';
  import CtkColorBoardEditor from './CtkColorBoardEditor.svelte';
  import { CtkDrawerDocument } from './ctkDrawerDocument';
  import {
    defaultCtk3Flags,
    encodeCtk3PageSourceAsync,
    type Ctk3DecodeWorkerLike,
    type Ctk3Operation,
    type Ctk3Page,
    type Ctk3PageFlags
  } from './ctk3Codec';
  import {
    clearCtkPageField,
    createLineClearedPage,
    grayscaleCtkPage,
    mirrorCtkPage,
    operationCells
  } from './ctkPageTools';
  import {
    ctkPageIndexFromArrowKey,
    ctkPageStripItems,
    type CtkPageStripItem
  } from './ctkPageNavigation';
  import {
    encodeFieldDocumentAsync,
    openFieldDocument
  } from './fieldInterchange';
  import {
    decodeValidatedRenderArtifact,
    quoteWebCommandToken
  } from './documentUtilityModel';
  import { fieldImportFailureMessageKey } from './fieldImportFailure';
  import { validateProductResultPayload } from './productResultPager';
  import {
    CTK3_FILE_ACCEPT,
    installGlobalDocumentDrop,
    installGlobalDocumentPaste,
    saveCtk3Source,
    sourceFromCtk3File
  } from './ctk3File';
  import WorkspaceShell from './WorkspaceShell.svelte';
  import {
    preferredWorkspaceLanguage,
    workspaceMessage,
    type WorkspaceLanguage,
    type WorkspaceMessageKey
  } from './workspaceI18n';
  import { workspaceViewFromWasm } from './workspaceRuntime';

  type CopyState = 'idle' | 'loading' | 'copied' | 'failed';
  export let initialDocument: string | undefined = undefined;
  export let viewerMode = false;
  export let workerFactory: (() => Worker) | null = null;

  const hostCapabilitySnapshot =
    getContext<HostCapabilitySnapshot>(HOST_CAPABILITY_SNAPSHOT_CONTEXT) ??
    sharedBrowserHostCapabilitySnapshot();
  const documentWorkerCount = automaticWorkerAuthority(
    hostCapabilitySnapshot
  ).workersEffective;
  const renderWorkerController = new WasmTerminalWorkerController(
    workerFactory,
    hostCapabilitySnapshot
  );

  let language: WorkspaceLanguage = 'en';
  let currentPage = blankPage(8);
  let documentModel = CtkDrawerDocument.fromPages(10, [currentPage]);
  let pageCount = 1;
  let pageIndex = 0;
  let importValue = '';
  let pendingImportSource: string | null = null;
  let importSummary = '';
  let importFailed = false;
  let importFailureKey: WorkspaceMessageKey = 'fieldImportInvalid';
  let importLoading = false;
  let pageLoading = false;
  let copyFormat: 'fumen' | 'ctk' = 'ctk';
  let copyState: CopyState = 'idle';
  let downloadState: CopyState = 'idle';
  let copyTimer = 0;
  let downloadTimer = 0;
  let pageLoadToken = 0;
  let lifecycleController = new AbortController();
  let copyController: AbortController | null = null;
  let downloadController: AbortController | null = null;
  let fileInput: HTMLInputElement;
  let pendingImportModel: CtkDrawerDocument | null = null;
  let previewPages = new Map<number, Ctk3Page>();
  let previewLoadToken = 0;
  let previewRequestKey = '';
  let mounted = false;
  let closed = false;
  let documentDragActive = false;
  let pageStripElement: HTMLDivElement;
  let renderFormat: 'png' | 'gif' = 'png';
  let renderPreparing = false;
  let renderObservedPayload: ClearraProductResultPayload | null = null;
  let renderArtifact: Extract<
    ClearraProductResultPayload,
    { content: { payload_kind: 'render-artifact' } }
  >['content']['payload'] | null = null;
  let renderArtifactUrl = '';
  let renderError = '';
  let renderGeneration = 0;
  let renderEncodeController: AbortController | null = null;
  let renderDocumentRevision = 0;
  let renderRequestedRevision = -1;

  $: currentHeight = Math.max(1, currentPage?.height ?? 1);
  $: renderWorkerController.setWorkerFactory(workerFactory);
  $: renderRuntimeView = workspaceViewFromWasm($wasmWorkerState);
  $: renderActive = renderPreparing ||
    renderRuntimeView.status === 'running' ||
    renderRuntimeView.status === 'cancelling';
  $: renderPayload = renderRuntimeView.response?.product_result_payload ?? null;
  $: if (renderPayload !== renderObservedPayload) acceptRenderPayload(renderPayload);
  $: hasImport = pendingImportSource !== null || importValue.trim().length > 0;
  $: pageStrip = ctkPageStripItems(pageCount, pageIndex);
  $: {
    const nextPreviewKey = pageStrip
      .filter((item): item is Extract<CtkPageStripItem, { kind: 'page' }> => item.kind === 'page')
      .map((item) => item.index)
      .join(',');
    if (mounted && nextPreviewKey !== previewRequestKey) {
      previewRequestKey = nextPreviewKey;
      void refreshPagePreviews();
    }
  }
  $: label = (
    key: Parameters<typeof workspaceMessage>[1],
    values: Record<string, string | number> = {}
  ) => workspaceMessage(language, key, values);

  onMount(() => {
    mounted = true;
    language = preferredWorkspaceLanguage(
      localStorage.getItem('clearra-language') ?? navigator.language
    );
    if (workerFactory) clearWasmTerminalResult();
    const viewerDocument = initialDocument ?? documentFromLocation();
    if (viewerDocument) {
      const source = viewerDocument;
      pendingImportSource = source;
      importSummary = documentSummary(source.length);
      requestAnimationFrame(() => {
        if (!closed) void importDocument(source);
      });
    } else {
      void refreshPagePreviews();
    }
    const handlePageHide = () => closeWorkspace();
    const handlePageShow = (event: PageTransitionEvent) => {
      if (event.persisted && closed) window.location.reload();
    };
    window.addEventListener('pagehide', handlePageHide);
    window.addEventListener('pageshow', handlePageShow);
    const removeDocumentPaste = installGlobalDocumentPaste({
      importSource: importPastedDocument,
      importFailed: () => {
        importFailureKey = 'fieldImportInvalid';
        importFailed = true;
      }
    });
    const removeDocumentDrop = installGlobalDocumentDrop({
      importSource: importPastedDocument,
      importFailed: () => {
        importFailureKey = 'fieldImportInvalid';
        importFailed = true;
      },
      dragActive: (active) => (documentDragActive = active)
    });
    return () => {
      window.removeEventListener('pagehide', handlePageHide);
      window.removeEventListener('pageshow', handlePageShow);
      removeDocumentPaste();
      removeDocumentDrop();
    };
  });

  onDestroy(closeWorkspace);

  function setLanguage(next: WorkspaceLanguage) {
    language = next;
    localStorage.setItem('clearra-language', next);
  }

  function setHeight(value: number) {
    const height = Math.max(1, Math.min(24, Math.trunc(value || 1)));
    const cells = Array(height * 10).fill(null);
    cells.splice(
      0,
      Math.min(cells.length, currentPage.cells.length),
      ...currentPage.cells.slice(0, cells.length)
    );
    updateCurrent({ ...currentPage, height, cells });
  }

  function updateCells(cells: Ctk3Page['cells']) {
    updateCurrent({ ...currentPage, height: currentHeight, cells });
  }

  function updateCurrent(page: Ctk3Page) {
    invalidateRenderResult();
    documentModel.updatePage(pageIndex, page);
    currentPage = page;
    previewPages = new Map(previewPages).set(pageIndex, clonePage(page));
  }

  function updateComment(comment: string) {
    updateCurrent({ ...currentPage, comment });
  }

  function updateFlag(
    flag: keyof Ctk3PageFlags,
    checked: boolean
  ) {
    updateCurrent({
      ...currentPage,
      flags: {
        ...defaultCtk3Flags(),
        ...(currentPage.flags ?? {}),
        [flag]: checked
      }
    });
  }

  function flagValue(flag: keyof Ctk3PageFlags): boolean {
    return currentPage.flags?.[flag] ?? defaultCtk3Flags()[flag];
  }

  function updateOperation(operation: Ctk3Operation | undefined) {
    updateCurrent({ ...currentPage, operation });
  }

  function mirrorCurrentPage() {
    updateCurrent(mirrorCtkPage(currentPage));
  }

  function removeCurrentColors() {
    updateCurrent(grayscaleCtkPage(currentPage));
  }

  function clearCurrentField() {
    updateCurrent(clearCtkPageField(currentPage));
  }

  function addLineClearedPage(grayscale: boolean) {
    invalidateRenderResult();
    const next = createLineClearedPage(currentPage, grayscale);
    documentModel.insertPage(pageIndex + 1, next);
    pageCount = documentModel.pageCount;
    pageIndex += 1;
    currentPage = next;
  }

  function previousPage() {
    void selectPage(pageIndex - 1);
  }

  function nextPage() {
    void selectPage(pageIndex + 1);
  }

  function handlePageArrow(event: KeyboardEvent) {
    const target = event.target;
    if (
      event.defaultPrevented ||
      event.isComposing ||
      event.ctrlKey ||
      event.metaKey ||
      event.altKey ||
      event.shiftKey ||
      !(target instanceof HTMLElement) ||
      isEditableElement(target)
    ) {
      return;
    }
    const nextIndex = ctkPageIndexFromArrowKey(event.key, pageIndex, pageCount);
    if (nextIndex === null || pageLoading) return;
    event.preventDefault();
    void selectPage(nextIndex);
  }

  async function selectPageFromPreview(index: number, anchor: HTMLButtonElement) {
    const anchorTop = anchor.getBoundingClientRect().top;
    await selectPage(index);
    await tick();
    await nextPaint();
    if (closed) return;
    const nextAnchor = pageStripElement?.querySelector<HTMLButtonElement>(
      `[data-page-index="${index}"]`
    );
    if (!nextAnchor) return;
    const offset = nextAnchor.getBoundingClientRect().top - anchorTop;
    if (Math.abs(offset) >= 0.5) window.scrollBy(0, offset);
    nextAnchor.focus({ preventScroll: true });
  }

  function isEditableElement(target: HTMLElement): boolean {
    return (
      target instanceof HTMLInputElement ||
      target instanceof HTMLTextAreaElement ||
      target instanceof HTMLSelectElement ||
      target.isContentEditable
    );
  }

  function addPage() {
    invalidateRenderResult();
    const next = blankPage(currentHeight);
    documentModel.insertPage(pageIndex + 1, next);
    pageCount = documentModel.pageCount;
    pageIndex += 1;
    currentPage = next;
  }

  function duplicatePage() {
    invalidateRenderResult();
    const duplicate = clonePage(currentPage);
    documentModel.insertPage(pageIndex + 1, duplicate);
    pageCount = documentModel.pageCount;
    pageIndex += 1;
    currentPage = duplicate;
  }

  function removePage() {
    invalidateRenderResult();
    if (pageCount === 1) {
      const next = blankPage(currentHeight);
      replaceDocument(CtkDrawerDocument.fromPages(10, [next]), next);
      pageIndex = 0;
      return;
    }
    documentModel.removePage(pageIndex);
    pageCount = documentModel.pageCount;
    void selectPage(Math.min(pageIndex, pageCount - 1), true);
  }

  async function selectPage(index: number, force = false) {
    if (closed) return;
    const nextIndex = Math.max(0, Math.min(pageCount - 1, index));
    if (!force && nextIndex === pageIndex) return;
    invalidateRenderResult();
    const token = ++pageLoadToken;
    pageLoading = true;
    try {
      const page = normalizeImportedPage(await documentModel.readPage(nextIndex));
      throwIfAborted(lifecycleController.signal);
      if (token !== pageLoadToken || closed) return;
      pageIndex = nextIndex;
      currentPage = page;
      previewPages = new Map(previewPages).set(nextIndex, clonePage(page));
    } catch (error) {
      if (!isAbortError(error) && token === pageLoadToken && !closed) {
        importFailureKey = fieldImportFailureMessageKey(error);
        importFailed = true;
      }
    } finally {
      if (token === pageLoadToken && !closed) pageLoading = false;
    }
  }

  async function importDocument(sourceOverride?: string) {
    if ((!sourceOverride && !hasImport) || importLoading || closed) return;
    invalidateRenderResult();
    importLoading = true;
    importFailed = false;
    await nextPaint();
    let nextModel: CtkDrawerDocument | null = null;
    try {
      throwIfAborted(lifecycleController.signal);
      const source = sourceOverride ?? pendingImportSource ?? importValue;
      const reader = openFieldDocument(source, {
        workers: documentWorkerCount,
        workerFactory: createCtkDocumentWorker,
        signal: lifecycleController.signal
      });
      nextModel = CtkDrawerDocument.fromReader(reader);
      pendingImportModel = nextModel;
      if (nextModel.width !== 10) throw new Error('width');
      const page = normalizeImportedPage(await nextModel.readPage(0));
      throwIfAborted(lifecycleController.signal);
      replaceDocument(nextModel, page);
      if (pendingImportModel === nextModel) pendingImportModel = null;
      nextModel = null;
      importSummary = documentSummary(source.length, pageCount);
      pendingImportSource = null;
      importValue = '';
      importFailed = false;
    } catch (error) {
      nextModel?.close();
      if (!isAbortError(error) && !closed) {
        importFailureKey = fieldImportFailureMessageKey(error);
        importFailed = true;
      }
    } finally {
      if (pendingImportModel === nextModel) pendingImportModel = null;
      if (!closed) importLoading = false;
    }
  }

  async function importPastedDocument(source: string) {
    while (importLoading && !closed) await nextPaint();
    if (!closed) await importDocument(source);
  }

  async function importCtk3File(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file) return;
    try {
      await importPastedDocument(await sourceFromCtk3File(file));
    } catch (error) {
      if (!closed) {
        importFailureKey = fieldImportFailureMessageKey(error);
        importFailed = true;
      }
    }
  }

  async function copyDocument() {
    if (copyState === 'loading' || closed) return;
    window.clearTimeout(copyTimer);
    copyController?.abort();
    const controller = new AbortController();
    copyController = controller;
    copyState = 'loading';
    await nextPaint();
    try {
      throwIfAborted(controller.signal);
      const encoded = await encodeDocument(copyFormat, controller.signal);
      throwIfAborted(controller.signal);
      await writeClipboardText(encoded, controller.signal);
      if (copyController !== controller || closed) return;
      setCopyState('copied');
    } catch (error) {
      if (controller.signal.aborted || isAbortError(error)) {
        if (!closed && copyController === controller) copyState = 'idle';
        return;
      }
      setCopyState('failed');
    } finally {
      if (copyController === controller) copyController = null;
    }
  }

  async function downloadDocument() {
    if (downloadState === 'loading' || closed) return;
    window.clearTimeout(downloadTimer);
    downloadController?.abort();
    const controller = new AbortController();
    downloadController = controller;
    downloadState = 'loading';
    await nextPaint();
    try {
      const encoded = await encodeDocument('ctk', controller.signal);
      throwIfAborted(controller.signal);
      saveCtk3Source(encoded, `clearra-${pageCount}-pages.ctk3`);
      if (downloadController !== controller || closed) return;
      setDownloadState('copied');
    } catch (error) {
      if (controller.signal.aborted || isAbortError(error)) {
        if (!closed && downloadController === controller) downloadState = 'idle';
        return;
      }
      setDownloadState('failed');
    } finally {
      if (downloadController === controller) downloadController = null;
    }
  }

  async function renderDocument() {
    if (!workerFactory || renderActive || closed) return;
    clearRenderResult();
    clearWasmTerminalResult();
    const controller = new AbortController();
    renderEncodeController = controller;
    renderPreparing = true;
    try {
      const source = await encodeDocument('ctk', controller.signal);
      throwIfAborted(controller.signal);
      updateWasmCommandText([
        'clearra utility render',
        '--format',
        'ctk3',
        '--document',
        quoteWebCommandToken(source),
        '--artifact-format',
        renderFormat,
        ...(renderFormat === 'png' ? ['--page', String(pageIndex + 1)] : [])
      ].join(' '));
      renderRequestedRevision = renderDocumentRevision;
      if (!renderWorkerController.run()) {
        throw new Error('the local browser render worker could not start');
      }
    } catch (error) {
      if (!controller.signal.aborted && !isAbortError(error)) {
        renderError = error instanceof Error ? error.message : String(error);
      }
    } finally {
      if (renderEncodeController === controller) renderEncodeController = null;
      renderPreparing = false;
    }
  }

  function cancelRender() {
    if (renderEncodeController && !renderEncodeController.signal.aborted) {
      renderEncodeController.abort(abortError('CTK render preparation was cancelled.'));
    }
    renderWorkerController.cancel();
  }

  function acceptRenderPayload(payload: ClearraProductResultPayload | null) {
    renderObservedPayload = payload;
    renderArtifact = null;
    renderError = '';
    revokeRenderArtifact();
    if (!payload) return;
    if (renderRequestedRevision !== renderDocumentRevision) return;
    const validationError = validateProductResultPayload(payload);
    if (validationError) {
      renderError = validationError;
      return;
    }
    if (payload.content.payload_kind !== 'render-artifact') {
      renderError = 'typed result does not match the CTK render utility';
      return;
    }
    renderArtifact = payload.content.payload;
    void prepareRenderArtifact(payload.content.payload, ++renderGeneration);
  }

  async function prepareRenderArtifact(
    payload: NonNullable<typeof renderArtifact>,
    generation: number
  ) {
    try {
      const bytes = await decodeValidatedRenderArtifact(payload);
      if (generation !== renderGeneration || closed) return;
      renderArtifactUrl = URL.createObjectURL(
        new Blob([bytes], { type: payload.media_type })
      );
    } catch (error) {
      if (generation === renderGeneration && !closed) {
        renderError = error instanceof Error ? error.message : String(error);
        renderArtifact = null;
      }
    }
  }

  function downloadRenderArtifact() {
    if (!renderArtifact || !renderArtifactUrl) return;
    const anchor = document.createElement('a');
    anchor.href = renderArtifactUrl;
    anchor.download = renderArtifact.filename;
    anchor.rel = 'noopener';
    anchor.click();
  }

  function clearRenderResult() {
    renderObservedPayload = null;
    renderRequestedRevision = -1;
    renderArtifact = null;
    renderError = '';
    revokeRenderArtifact();
  }

  function invalidateRenderResult() {
    renderDocumentRevision += 1;
    if (!workerFactory) return;
    if (renderActive) cancelRender();
    clearWasmTerminalResult();
    clearRenderResult();
  }

  function revokeRenderArtifact() {
    renderGeneration += 1;
    if (renderArtifactUrl) URL.revokeObjectURL(renderArtifactUrl);
    renderArtifactUrl = '';
  }

  async function encodeDocument(
    format: 'fumen' | 'ctk',
    signal: AbortSignal
  ): Promise<string> {
    const original = format === 'ctk' ? documentModel.originalCtk : null;
    if (original) return original;
    if (format === 'ctk') {
      return encodeCtk3PageSourceAsync(documentModel, {
        workers: documentWorkerCount,
        workerFactory: createCtkDocumentWorker,
        signal
      });
    }
    const materialized = await documentModel.materialize(signal);
    throwIfAborted(signal);
    return encodeFieldDocumentAsync(materialized, format, {
      signal,
      workers: documentWorkerCount
    });
  }

  function setCopyState(next: 'copied' | 'failed') {
    copyState = next;
    window.clearTimeout(copyTimer);
    copyTimer = window.setTimeout(() => {
      copyState = 'idle';
    }, 1600);
  }

  function setDownloadState(next: 'copied' | 'failed') {
    downloadState = next;
    window.clearTimeout(downloadTimer);
    downloadTimer = window.setTimeout(() => {
      downloadState = 'idle';
    }, 1600);
  }

  function nextPaint(): Promise<void> {
    return new Promise((resolve) => requestAnimationFrame(() => resolve()));
  }

  function replaceDocument(next: CtkDrawerDocument, page: Ctk3Page) {
    throwIfAborted(lifecycleController.signal);
    invalidateRenderResult();
    documentModel.close();
    documentModel = next;
    pageCount = next.pageCount;
    pageIndex = 0;
    currentPage = page;
    previewPages = new Map([[0, clonePage(page)]]);
    previewRequestKey = '';
    previewLoadToken += 1;
    pageLoadToken += 1;
    pageLoading = false;
  }

  function closeWorkspace() {
    if (closed) return;
    closed = true;
    pageLoadToken += 1;
    previewLoadToken += 1;
    clearTimeout(copyTimer);
    clearTimeout(downloadTimer);
    const error = abortError('CTK workspace was closed.');
    if (!lifecycleController.signal.aborted) lifecycleController.abort(error);
    if (copyController && !copyController.signal.aborted) {
      copyController.abort(error);
    }
    if (downloadController && !downloadController.signal.aborted) {
      downloadController.abort(error);
    }
    if (renderEncodeController && !renderEncodeController.signal.aborted) {
      renderEncodeController.abort(error);
    }
    renderWorkerController.dispose();
    revokeRenderArtifact();
    if (workerFactory) clearWasmTerminalResult();
    pendingImportModel?.close();
    pendingImportModel = null;
    documentModel.close();
    importLoading = false;
    pageLoading = false;
    copyState = 'idle';
    downloadState = 'idle';
    renderPreparing = false;
  }

  function throwIfAborted(signal: AbortSignal) {
    if (!signal.aborted) return;
    if (signal.reason instanceof Error) throw signal.reason;
    throw abortError('CTK operation was aborted.');
  }

  function isAbortError(error: unknown): boolean {
    return error instanceof Error && error.name === 'AbortError';
  }

  function abortError(message: string): Error {
    const error = new Error(message);
    error.name = 'AbortError';
    return error;
  }

  function handleImportInput(event: Event) {
    pendingImportSource = null;
    importSummary = '';
    importValue = (event.currentTarget as HTMLTextAreaElement).value;
    importFailureKey = 'fieldImportInvalid';
    importFailed = false;
  }

  function handleImportPaste(event: ClipboardEvent) {
    const source = event.clipboardData?.getData('text/plain') ?? '';
    if (source.length < 64 * 1024) return;
    event.preventDefault();
    pendingImportSource = source;
    importValue = '';
    importSummary = documentSummary(source.length);
    importFailureKey = 'fieldImportInvalid';
    importFailed = false;
  }

  function createCtkDocumentWorker(): Ctk3DecodeWorkerLike {
    return new Worker(
      new URL('./ctkDocumentDecodeWorker.ts', import.meta.url),
      {
        type: 'module',
        name: 'clearra-ctk-document'
      }
    ) as unknown as Ctk3DecodeWorkerLike;
  }

  function documentFromLocation(): string | undefined {
    const url = new URL(window.location.href);
    const named =
      url.searchParams.get('ctk') ??
      url.searchParams.get('fumen') ??
      url.searchParams.get('document');
    if (named) return named;
    const raw = url.search.slice(1);
    if (!/^(?:v11(?:0|5)@|ctk3(?:b_|_|@))/i.test(raw)) return undefined;
    try {
      return decodeURIComponent(raw);
    } catch {
      return raw;
    }
  }

  function documentSummary(characters: number, pages?: number): string {
    const size = characters >= 1024 * 1024
      ? `${(characters / 1024 / 1024).toFixed(1)} MiB`
      : `${Math.max(1, Math.ceil(characters / 1024))} KiB`;
    return pages
      ? `CTK3 · ${pages.toLocaleString()} pages · ${size}`
      : `CTK3 · ${size}`;
  }

  async function refreshPagePreviews() {
    if (closed) return;
    const token = ++previewLoadToken;
    const indices = pageStrip
      .filter((item): item is Extract<CtkPageStripItem, { kind: 'page' }> => item.kind === 'page')
      .map((item) => item.index)
      .sort((left, right) => left - right);
    const next = new Map<number, Ctk3Page>();
    for (const index of indices) {
      const cached = previewPages.get(index);
      if (cached) next.set(index, cached);
    }
    const missing = indices.filter((index) => !next.has(index));
    try {
      for (let cursor = 0; cursor < missing.length;) {
        const start = missing[cursor];
        let end = cursor + 1;
        while (end < missing.length && missing[end] === missing[end - 1] + 1) end += 1;
        const pages = await documentModel.readPages(
          start,
          end - cursor,
          lifecycleController.signal
        );
        throwIfAborted(lifecycleController.signal);
        if (token !== previewLoadToken || closed) return;
        for (let offset = 0; offset < pages.length; offset += 1) {
          next.set(start + offset, normalizeImportedPage(pages[offset]));
        }
        cursor = end;
      }
      if (token === previewLoadToken && !closed) previewPages = next;
    } catch (error) {
      if (!isAbortError(error) && token === previewLoadToken && !closed) {
        previewPages = new Map([[pageIndex, clonePage(currentPage)]]);
      }
    }
  }

  function previewCells(page: Ctk3Page): Ctk3Page['cells'] {
    const height = Math.max(1, page.height);
    const cells = Array(height * 10).fill(null) as Ctk3Page['cells'];
    cells.splice(0, Math.min(cells.length, page.cells.length), ...page.cells.slice(0, cells.length));
    if (page.operation) {
      for (const cell of operationCells(page.operation)) {
        if (cell.x >= 0 && cell.x < 10 && cell.y >= 0 && cell.y < height) {
          cells[cell.y * 10 + cell.x] = page.operation.piece;
        }
      }
    }
    const display: Ctk3Page['cells'] = [];
    for (let y = height - 1; y >= 0; y -= 1) {
      display.push(...cells.slice(y * 10, y * 10 + 10));
    }
    return display;
  }

  function blankPage(height: number): Ctk3Page {
    return {
      height,
      cells: Array(height * 10).fill(null),
      comment: '',
      flags: defaultCtk3Flags()
    };
  }

  function clonePage(page: Ctk3Page): Ctk3Page {
    return {
      ...page,
      cells: page.cells.slice(),
      flags: { ...(page.flags ?? {}) },
      operation: page.operation ? { ...page.operation } : undefined,
      garbage: page.garbage?.slice()
    };
  }

  function normalizeImportedPage(page: Ctk3Page): Ctk3Page {
    const operationHeight = page.operation
      ? operationCells(page.operation).reduce(
          (maximum, cell) => Math.max(maximum, cell.y + 1),
          0
        )
      : 0;
    const height = Math.max(1, page.height || 1, operationHeight);
    if (height > 24) throw new Error('height');
    const cells = Array(height * 10).fill(null);
    cells.splice(
      0,
      Math.min(cells.length, page.cells.length),
      ...page.cells.slice(0, cells.length)
    );
    return {
      ...page,
      height,
      cells,
      flags: { ...defaultCtk3Flags(), ...(page.flags ?? {}) }
    };
  }
</script>

<svelte:head>
  <title>{label('ctkDrawer')} · Clearra</title>
  <meta name="description" content="Multi-page Fumen and CTK field editor" />
  {#if viewerMode}
    <meta property="og:title" content="Clearra CTK Viewer" />
    <meta property="og:description" content="Open this Fumen or CTK3 document in Clearra." />
  {/if}
</svelte:head>

<svelte:window on:keydown={handlePageArrow} />

<WorkspaceShell
  activeMode="ctk"
  {language}
  active={false}
  statusLabel={label('idle')}
  workspaceLabel={label('ctkDrawer')}
  dimensionLabel={label('fieldHeight')}
  dimensionValue={currentHeight}
  dimensionMin={1}
  dimensionMax={24}
  cancelLabel={label('cancel')}
  runLabel={label('run')}
  showActions={false}
  on:language={(event) => setLanguage(event.detail)}
  on:dimension={(event) => setHeight(event.detail)}
>
  <section
    slot="editor"
    class="drawer-board"
    class:document-drag-active={documentDragActive}
  >
    {#if documentDragActive}
      <div class="document-drop-overlay" role="status" aria-live="polite">
        <FileUp size={22} />
        <strong>{label('ctkDropDocument')}</strong>
        <span>{label('ctkDropDocumentHelp')}</span>
      </div>
    {/if}
    <header class="section-heading">
      <div>
        <span>{label('ctkPage')}</span>
        <strong>{label('ctkPageCount', { current: pageIndex + 1, total: pageCount })}</strong>
      </div>
      <div class="page-actions" role="toolbar" aria-label={label('ctkPages')}>
        <button type="button" disabled={pageLoading || pageIndex === 0} title={label('previousPage')} on:click={previousPage}>
          <ChevronLeft size={16} />
        </button>
        <button type="button" disabled={pageLoading || pageIndex >= pageCount - 1} title={label('nextPage')} on:click={nextPage}>
          <ChevronRight size={16} />
        </button>
        <button type="button" title={label('addPage')} on:click={addPage}>
          <Plus size={16} />
        </button>
        <button type="button" title={label('duplicatePage')} on:click={duplicatePage}>
          <Copy size={15} />
        </button>
        <button type="button" title={label('deletePage')} on:click={removePage}>
          <Trash2 size={15} />
        </button>
      </div>
    </header>

    <CtkColorBoardEditor
      height={currentHeight}
      cells={currentPage.cells}
      operation={currentPage.operation}
      {language}
      on:change={(event) => updateCells(event.detail)}
      on:mirror={mirrorCurrentPage}
      on:lineclear={(event) => addLineClearedPage(event.detail.grayscale)}
      on:grayscale={removeCurrentColors}
      on:clear={clearCurrentField}
      on:operation={(event) => updateOperation(event.detail)}
    />

    <div bind:this={pageStripElement} class="page-strip" aria-label={label('ctkPages')}>
      {#each pageStrip as item (item.kind === 'page' ? `page-${item.index}` : `gap-${item.key}`)}
        {#if item.kind === 'gap'}
          <span class="page-gap" aria-hidden="true">…</span>
        {:else}
          {@const preview = previewPages.get(item.index)}
          <button
            type="button"
            class="page-preview"
            class:active={item.index === pageIndex}
            aria-current={item.index === pageIndex ? 'page' : undefined}
            disabled={pageLoading}
            data-page-index={item.index}
            title={`${label('ctkPage')} ${item.index + 1}`}
            on:click={(event) => void selectPageFromPreview(
              item.index,
              event.currentTarget as HTMLButtonElement
            )}
          >
            <span class="page-number">{item.index + 1}</span>
            {#if preview}
              <span
                class="page-mini-board"
                style={`--preview-height: ${Math.max(1, preview.height)}`}
                aria-hidden="true"
              >
                {#each previewCells(preview) as cell}
                  <span class={`preview-cell piece-${cell ?? 'empty'}`}></span>
                {/each}
              </span>
            {:else}
              <span class="page-preview-placeholder" aria-hidden="true"></span>
            {/if}
          </button>
        {/if}
      {/each}
    </div>
  </section>

  <section slot="controls" class="drawer-controls">
    <div class="control-section">
      <h2>{label('ctkImport')}</h2>
      <label class="field-control">
        <span>{label('fieldImport')}</span>
        <textarea
          rows="3"
          value={importValue}
          placeholder="v115@... / ctk3_..."
          spellcheck="false"
          aria-invalid={importFailed}
          on:input={handleImportInput}
          on:paste={handleImportPaste}
        ></textarea>
      </label>
      {#if importSummary}
        <p class="import-summary">{importSummary}</p>
      {/if}
      <button
        class="command-button"
        type="button"
        disabled={!hasImport || importLoading}
        aria-busy={importLoading}
        on:click={() => void importDocument()}
      >
        {#if importLoading}
          <span class="spinner"><LoaderCircle size={15} /></span>
        {:else}
          <Upload size={15} />
        {/if}
        {label('loadDocument')}
      </button>
      <input
        class="file-input"
        bind:this={fileInput}
        type="file"
        accept={CTK3_FILE_ACCEPT}
        on:change={importCtk3File}
      />
      <button
        class="command-button"
        type="button"
        disabled={importLoading}
        on:click={() => fileInput?.click()}
      >
        <FileUp size={15} />
        {label('loadCtk3File')}
      </button>
      {#if importFailed}
        <p class="error" role="alert">{label(importFailureKey)}</p>
      {/if}
    </div>

    <div class="control-section">
      <h2>{label('ctkPageMetadata')}</h2>
      <label class="field-control">
        <span>{label('ctkComment')}</span>
        <textarea
          rows="3"
          value={currentPage.comment ?? ''}
          on:input={(event) => updateComment((event.currentTarget as HTMLTextAreaElement).value)}
        ></textarea>
      </label>
      <div class="flag-grid">
        {#each ['lock', 'colorize', 'rise', 'quiz'] as flag}
          <label>
            <input
              type="checkbox"
              checked={flagValue(flag as keyof Ctk3PageFlags)}
              on:change={(event) => updateFlag(
                flag as keyof Ctk3PageFlags,
                (event.currentTarget as HTMLInputElement).checked
              )}
            />
            <span>{label(`ctkFlag${flag[0].toUpperCase()}${flag.slice(1)}` as Parameters<typeof workspaceMessage>[1])}</span>
            <button
              type="button"
              class="flag-help"
              aria-label={label(`ctkFlag${flag[0].toUpperCase()}${flag.slice(1)}Help` as Parameters<typeof workspaceMessage>[1])}
              data-tooltip={label(`ctkFlag${flag[0].toUpperCase()}${flag.slice(1)}Help` as Parameters<typeof workspaceMessage>[1])}
            ><HelpCircle size={13} strokeWidth={1.8} /></button>
          </label>
        {/each}
      </div>
    </div>

    <div class="control-section export-section">
      <h2>{label('ctkExport')}</h2>
      <div class="export-row">
        <div class="segments" role="group" aria-label={label('solutionCopyFormat')}>
          <button type="button" class:active={copyFormat === 'fumen'} on:click={() => (copyFormat = 'fumen')}>Fumen</button>
          <button type="button" class:active={copyFormat === 'ctk'} on:click={() => (copyFormat = 'ctk')}>CTK3</button>
        </div>
        <button
          type="button"
          class="copy-document"
          disabled={copyState === 'loading'}
          aria-busy={copyState === 'loading'}
          on:click={copyDocument}
        >
          {#if copyState === 'loading'}
            <span class="spinner"><LoaderCircle size={15} /></span>
          {:else if copyState === 'copied'}
            <Check size={15} />
          {:else if copyState === 'failed'}
            <AlertTriangle size={15} />
          {:else}
            <ClipboardCopy size={15} />
          {/if}
          {label(copyState === 'loading' ? 'copyAllPending' : 'copyDocument')}
        </button>
        {#if copyFormat === 'ctk'}
          <button
            type="button"
            class="copy-document"
            disabled={downloadState === 'loading'}
            aria-busy={downloadState === 'loading'}
            on:click={downloadDocument}
          >
            {#if downloadState === 'loading'}
              <span class="spinner"><LoaderCircle size={15} /></span>
            {:else if downloadState === 'copied'}
              <Check size={15} />
            {:else if downloadState === 'failed'}
              <AlertTriangle size={15} />
            {:else}
              <Download size={15} />
            {/if}
            {label('downloadCtk3File')}
          </button>
        {/if}
      </div>
      {#if copyState === 'failed' || downloadState === 'failed'}
        <p class="error" role="alert">{label('documentCopyFailed')}</p>
      {/if}
    </div>

    {#if workerFactory}
      <div class="control-section render-section">
        <h2>{label('utilityRender')}</h2>
        <p class="render-help">
          {language === 'ko'
            ? `브라우저의 로컬 WASM에서만 실행합니다. PNG는 현재 ${pageIndex + 1}페이지, GIF는 전체 문서를 렌더합니다.`
            : `Runs only in local browser WASM. PNG renders current page ${pageIndex + 1}; GIF renders the full document.`}
        </p>
        <div class="export-row">
          <div class="segments" role="group" aria-label={language === 'ko' ? '렌더 형식' : 'Render format'}>
            <button type="button" class:active={renderFormat === 'png'} disabled={renderActive} on:click={() => (renderFormat = 'png')}>PNG</button>
            <button type="button" class:active={renderFormat === 'gif'} disabled={renderActive} on:click={() => (renderFormat = 'gif')}>GIF</button>
          </div>
          <button
            type="button"
            class="copy-document"
            disabled={importLoading || pageLoading}
            aria-busy={renderActive}
            on:click={() => renderActive ? cancelRender() : void renderDocument()}
          >
            {#if renderActive}
              <span class="spinner"><LoaderCircle size={15} /></span>
              {label('cancel')}
            {:else}
              <Image size={15} />
              {label('utilityRender')}
            {/if}
          </button>
        </div>
      </div>
    {/if}
  </section>

  <section
    slot="result"
    class="render-result"
    class:hidden={!workerFactory || !(renderActive || renderArtifact || renderError || renderRuntimeView.error)}
    aria-live="polite"
  >
    {#if workerFactory}
      <h2>{label('utilityRender')}</h2>
      {#if renderArtifact && renderArtifactUrl}
        <figure>
          <img src={renderArtifactUrl} alt={language === 'ko' ? 'CTK 로컬 렌더 결과' : 'Local CTK render result'} />
          <figcaption>
            {renderArtifact.filename} · {renderArtifact.byte_length} bytes · SHA-256 {renderArtifact.sha256}
          </figcaption>
        </figure>
        <button class="render-download" type="button" on:click={downloadRenderArtifact}>
          <Download size={15} />{language === 'ko' ? '렌더 다운로드' : 'Download render'}
        </button>
      {:else if renderError || renderRuntimeView.error}
        <p class="error" role="alert">{renderError || renderRuntimeView.error}</p>
      {:else}
        <p class="render-pending">{language === 'ko' ? '로컬 렌더를 준비하고 있습니다.' : 'Preparing the local render.'}</p>
      {/if}
    {/if}
  </section>
</WorkspaceShell>

<style>
  .drawer-board, .drawer-controls { min-width: 0; }
  .drawer-board { position: relative; }
  .document-drag-active { isolation: isolate; }
  .document-drop-overlay {
    align-content: center;
    background: rgba(232, 247, 244, .96);
    border: 2px dashed #16877d;
    border-radius: 8px;
    color: #075f58;
    display: grid;
    gap: 6px;
    inset: 0;
    justify-items: center;
    min-height: 220px;
    padding: 24px;
    position: absolute;
    text-align: center;
    z-index: 8;
  }
  .document-drop-overlay strong { font-size: 14px; }
  .document-drop-overlay span { color: #42645e; font-size: 11px; }
  .section-heading {
    align-items: center;
    display: flex;
    gap: 16px;
    justify-content: space-between;
    margin-bottom: 10px;
  }
  .section-heading > div:first-child { display: grid; gap: 3px; }
  .section-heading span { color: #68736f; font-size: 10px; font-weight: 700; }
  .section-heading strong { color: #26322e; font-size: 14px; }
  .page-actions { display: flex; gap: 5px; }
  .page-actions button {
    align-items: center;
    background: #fff;
    border: 1px solid #cbd3ce;
    border-radius: 5px;
    color: #42514c;
    cursor: pointer;
    display: inline-flex;
    height: 34px;
    justify-content: center;
    padding: 0;
    width: 34px;
  }
  button:disabled { cursor: default; opacity: .4; }
  .render-help { color: #65716c; font-size: 11px; line-height: 1.5; margin: 0; }
  .render-result { margin: 0 auto; max-width: 1460px; padding: 8px 24px 40px; }
  .render-result.hidden { display: none; }
  .render-result h2 { font-size: 17px; margin: 0 0 14px; }
  .render-result figure, .render-pending, .render-result > .error {
    background: #fff;
    border: 1px solid #d5dcd7;
    border-radius: 7px;
    margin: 0;
    padding: 14px 18px;
  }
  .render-result figure { display: grid; gap: 12px; justify-items: center; }
  .render-result img { image-rendering: pixelated; max-height: 620px; max-width: 100%; }
  .render-result figcaption { color: #65716c; font-size: 11px; overflow-wrap: anywhere; }
  .render-download {
    align-items: center;
    background: #fff;
    border: 1px solid #cbd3ce;
    border-radius: 5px;
    color: #26322e;
    cursor: pointer;
    display: inline-flex;
    gap: 6px;
    margin-top: 12px;
    min-height: 34px;
    padding: 6px 10px;
  }
  .page-strip {
    display: grid;
    gap: 8px;
    grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
    margin-top: 14px;
    max-width: 100%;
    min-width: 0;
    overflow-anchor: none;
    padding: 2px;
  }
  .page-preview {
    align-items: center;
    background: #f3f5f4;
    border: 1px solid #cbd3ce;
    border-radius: 5px;
    color: #596560;
    cursor: pointer;
    display: grid;
    font: inherit;
    gap: 5px;
    justify-items: center;
    min-height: 82px;
    min-width: 0;
    padding: 6px;
    width: 100%;
  }
  .page-preview.active {
    background: #e8f5f3;
    border-color: #0f766e;
    box-shadow: inset 0 0 0 1px #0f766e;
    color: #0f665f;
  }
  .page-number {
    font-size: 10px;
    font-weight: 750;
    line-height: 1;
  }
  .page-mini-board {
    background: #172320;
    border: 2px solid #172320;
    display: grid;
    grid-template-columns: repeat(10, minmax(0, 1fr));
    grid-template-rows: repeat(var(--preview-height), minmax(0, 1fr));
    max-height: 72px;
    overflow: hidden;
    width: min(100%, calc(72px * 10 / var(--preview-height)));
    aspect-ratio: 10 / var(--preview-height);
  }
  .preview-cell {
    background: #172320;
    min-height: 0;
    min-width: 0;
  }
  .preview-cell.piece-I { background: #55cbd3; }
  .preview-cell.piece-O { background: #f1ce47; }
  .preview-cell.piece-T { background: #b66bd1; }
  .preview-cell.piece-S { background: #64c67a; }
  .preview-cell.piece-Z { background: #ec6969; }
  .preview-cell.piece-J { background: #5c86df; }
  .preview-cell.piece-L { background: #e89a46; }
  .preview-cell.piece-G { background: #858d89; }
  .page-preview-placeholder {
    animation: preview-pulse 1.2s ease-in-out infinite alternate;
    background: #dde3e0;
    height: 42px;
    width: min(100%, 76px);
  }
  .page-gap {
    align-items: center;
    color: #77827d;
    display: flex;
    font-size: 12px;
    justify-content: center;
    min-height: 82px;
  }
  @keyframes preview-pulse {
    from { opacity: .45; }
    to { opacity: .9; }
  }
  .drawer-controls { display: grid; gap: 20px; }
  .control-section {
    border-bottom: 1px solid #dce2de;
    display: grid;
    gap: 10px;
    padding-bottom: 20px;
  }
  .control-section:last-child { border-bottom: 0; padding-bottom: 0; }
  h2 { color: #27342f; font-size: 13px; margin: 0; }
  .field-control { display: grid; gap: 5px; }
  .field-control > span {
    color: #68736f;
    font-size: 10px;
    font-weight: 700;
  }
  textarea {
    background: #fff;
    border: 1px solid #cbd3ce;
    border-radius: 5px;
    color: #26322e;
    font: inherit;
    font-size: 12px;
    padding: 8px 10px;
    width: 100%;
  }
  textarea { line-height: 1.5; resize: vertical; }
  textarea:focus, input:focus {
    border-color: #16877d;
    box-shadow: 0 0 0 3px #16877d1f;
    outline: 0;
  }
  textarea[aria-invalid="true"] { border-color: #c95f46; }
  .command-button, .copy-document {
    align-items: center;
    background: #fff;
    border: 1px solid #aebbb5;
    border-radius: 5px;
    color: #34443e;
    cursor: pointer;
    display: inline-flex;
    font: inherit;
    font-size: 10px;
    font-weight: 750;
    gap: 7px;
    justify-content: center;
    min-height: 34px;
    padding: 0 11px;
  }
  .command-button { justify-self: start; }
  .file-input { display: none; }
  .flag-grid {
    display: grid;
    gap: 8px 14px;
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .flag-grid label {
    align-items: center;
    color: #45534e;
    display: flex;
    font-size: 11px;
    gap: 7px;
  }
  input[type="checkbox"] { accent-color: #16877d; height: 16px; width: 16px; }
  .flag-help {
    align-items: center;
    background: transparent;
    border: 0;
    color: #77827d;
    cursor: help;
    display: inline-flex;
    height: 22px;
    justify-content: center;
    padding: 0;
    position: relative;
    width: 22px;
  }
  .flag-help::after {
    background: #1d2926;
    border-radius: 4px;
    bottom: calc(100% + 7px);
    color: #fff;
    content: attr(data-tooltip);
    font-size: 10px;
    font-weight: 500;
    left: 50%;
    line-height: 1.45;
    max-width: 230px;
    opacity: 0;
    padding: 7px 8px;
    pointer-events: none;
    position: absolute;
    transform: translate(-50%, 3px);
    transition: opacity 100ms ease, transform 100ms ease;
    visibility: hidden;
    width: max-content;
    z-index: 10;
  }
  .flag-help:hover::after, .flag-help:focus-visible::after {
    opacity: 1;
    transform: translate(-50%, 0);
    visibility: visible;
  }
  .export-row {
    align-items: center;
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .segments {
    border: 1px solid #aebbb5;
    border-radius: 5px;
    display: grid;
    grid-template-columns: repeat(2, minmax(68px, 1fr));
    overflow: hidden;
  }
  .segments button {
    background: #fff;
    border: 0;
    color: #53615c;
    cursor: pointer;
    font: inherit;
    font-size: 10px;
    font-weight: 750;
    min-height: 34px;
    padding: 0 10px;
  }
  .segments button + button { border-left: 1px solid #aebbb5; }
  .segments button.active { background: #16877d; color: #fff; }
  .copy-document { min-width: 120px; }
  .error, .import-summary { font-size: 10px; line-height: 1.45; margin: 0; }
  .error { color: #a24735; }
  .import-summary { color: #52615c; }
  .spinner { animation: spin .8s linear infinite; display: inline-flex; }
  @keyframes spin { to { transform: rotate(360deg); } }

  @media (max-width: 560px) {
    .section-heading { align-items: flex-start; }
    .page-actions { display: grid; grid-template-columns: repeat(5, 32px); }
    .page-actions button { height: 32px; width: 32px; }
    .render-result { padding-left: 16px; padding-right: 16px; }
    .flag-grid { grid-template-columns: 1fr; }
    .export-row { align-items: stretch; display: grid; }
    .copy-document { width: 100%; }
  }
</style>
