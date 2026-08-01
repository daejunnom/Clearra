<!-- SRP rationale: this component has one change reason: the complete CTK document-editing workspace interaction contract. -->
<script lang="ts">
  import {
    AlertTriangle,
    Check,
    ChevronLeft,
    ChevronRight,
    ClipboardCopy,
    Copy,
    HelpCircle,
    LoaderCircle,
    Plus,
    Trash2,
    Upload
  } from '@lucide/svelte';
  import { onDestroy, onMount } from 'svelte';

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
    encodeFieldDocumentAsync,
    openFieldDocument
  } from './fieldInterchange';
  import WorkspaceShell from './WorkspaceShell.svelte';
  import {
    preferredWorkspaceLanguage,
    workspaceMessage,
    type WorkspaceLanguage
  } from './workspaceI18n';

  type CopyState = 'idle' | 'loading' | 'copied' | 'failed';
  type PageStripItem =
    | { kind: 'page'; index: number }
    | { kind: 'gap'; key: string };

  export let initialDocument: string | undefined = undefined;
  export let viewerMode = false;

  let language: WorkspaceLanguage = 'en';
  let currentPage = blankPage(8);
  let documentModel = CtkDrawerDocument.fromPages(10, [currentPage]);
  let pageCount = 1;
  let pageIndex = 0;
  let importValue = '';
  let pendingImportSource: string | null = null;
  let importSummary = '';
  let importFailed = false;
  let importLoading = false;
  let pageLoading = false;
  let copyFormat: 'fumen' | 'ctk' = 'ctk';
  let copyState: CopyState = 'idle';
  let copyTimer = 0;
  let pageLoadToken = 0;
  let lifecycleController = new AbortController();
  let copyController: AbortController | null = null;
  let pendingImportModel: CtkDrawerDocument | null = null;
  let previewPages = new Map<number, Ctk3Page>();
  let previewLoadToken = 0;
  let previewRequestKey = '';
  let mounted = false;
  let closed = false;

  $: currentHeight = Math.max(1, currentPage?.height ?? 1);
  $: hasImport = pendingImportSource !== null || importValue.trim().length > 0;
  $: pageStrip = pageStripItems(pageCount, pageIndex);
  $: {
    const nextPreviewKey = pageStrip
      .filter((item): item is Extract<PageStripItem, { kind: 'page' }> => item.kind === 'page')
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
    return () => {
      window.removeEventListener('pagehide', handlePageHide);
      window.removeEventListener('pageshow', handlePageShow);
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

  function addPage() {
    const next = blankPage(currentHeight);
    documentModel.insertPage(pageIndex + 1, next);
    pageCount = documentModel.pageCount;
    pageIndex += 1;
    currentPage = next;
  }

  function duplicatePage() {
    const duplicate = clonePage(currentPage);
    documentModel.insertPage(pageIndex + 1, duplicate);
    pageCount = documentModel.pageCount;
    pageIndex += 1;
    currentPage = duplicate;
  }

  function removePage() {
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
        importFailed = true;
      }
    } finally {
      if (token === pageLoadToken && !closed) pageLoading = false;
    }
  }

  async function importDocument(sourceOverride?: string) {
    if ((!sourceOverride && !hasImport) || importLoading || closed) return;
    importLoading = true;
    importFailed = false;
    await nextPaint();
    let nextModel: CtkDrawerDocument | null = null;
    try {
      throwIfAborted(lifecycleController.signal);
      const source = sourceOverride ?? pendingImportSource ?? importValue;
      const reader = openFieldDocument(source, {
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
      if (!isAbortError(error) && !closed) importFailed = true;
    } finally {
      if (pendingImportModel === nextModel) pendingImportModel = null;
      if (!closed) importLoading = false;
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
      const original = copyFormat === 'ctk' ? documentModel.originalCtk : null;
      let encoded = original;
      if (!encoded) {
        if (copyFormat === 'ctk') {
          encoded = await encodeCtk3PageSourceAsync(documentModel, {
            workerFactory: createCtkDocumentWorker,
            signal: controller.signal
          });
        } else {
          const materialized = await documentModel.materialize(controller.signal);
          throwIfAborted(controller.signal);
          encoded = await encodeFieldDocumentAsync(
            materialized,
            copyFormat,
            { signal: controller.signal }
          );
        }
      }
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

  function setCopyState(next: 'copied' | 'failed') {
    copyState = next;
    window.clearTimeout(copyTimer);
    copyTimer = window.setTimeout(() => {
      copyState = 'idle';
    }, 1600);
  }

  function nextPaint(): Promise<void> {
    return new Promise((resolve) => requestAnimationFrame(() => resolve()));
  }

  function replaceDocument(next: CtkDrawerDocument, page: Ctk3Page) {
    throwIfAborted(lifecycleController.signal);
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
    const error = abortError('CTK workspace was closed.');
    if (!lifecycleController.signal.aborted) lifecycleController.abort(error);
    if (copyController && !copyController.signal.aborted) {
      copyController.abort(error);
    }
    pendingImportModel?.close();
    pendingImportModel = null;
    documentModel.close();
    importLoading = false;
    pageLoading = false;
    copyState = 'idle';
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
    importFailed = false;
  }

  function handleImportPaste(event: ClipboardEvent) {
    const source = event.clipboardData?.getData('text/plain') ?? '';
    if (source.length < 64 * 1024) return;
    event.preventDefault();
    pendingImportSource = source;
    importValue = '';
    importSummary = documentSummary(source.length);
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
      .filter((item): item is Extract<PageStripItem, { kind: 'page' }> => item.kind === 'page')
      .map((item) => item.index)
      .sort((left, right) => left - right);
    const next = new Map<number, Ctk3Page>();
    try {
      for (let cursor = 0; cursor < indices.length;) {
        const start = indices[cursor];
        let end = cursor + 1;
        while (end < indices.length && indices[end] === indices[end - 1] + 1) end += 1;
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

  function pageStripItems(total: number, current: number): PageStripItem[] {
    const nearbyPageRadius = 10;
    if (total <= nearbyPageRadius * 2 + 5) {
      return Array.from({ length: total }, (_, index) => ({
        kind: 'page' as const,
        index
      }));
    }
    const indices = new Set<number>([0, total - 1]);
    for (
      let index = Math.max(0, current - nearbyPageRadius);
      index <= Math.min(total - 1, current + nearbyPageRadius);
      index += 1
    ) {
      indices.add(index);
    }
    const sorted = [...indices].sort((left, right) => left - right);
    const items: PageStripItem[] = [];
    for (let index = 0; index < sorted.length; index += 1) {
      if (index > 0 && sorted[index] - sorted[index - 1] > 1) {
        items.push({ kind: 'gap', key: `${sorted[index - 1]}-${sorted[index]}` });
      }
      items.push({ kind: 'page', index: sorted[index] });
    }
    return items;
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
  <section slot="editor" class="drawer-board">
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

    <div class="page-strip" aria-label={label('ctkPages')}>
      {#each pageStrip as item}
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
            title={`${label('ctkPage')} ${item.index + 1}`}
            on:click={() => selectPage(item.index)}
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
      {#if importFailed}
        <p class="error" role="alert">{label('fieldImportInvalid')}</p>
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
      </div>
      {#if copyState === 'failed'}
        <p class="error" role="alert">{label('documentCopyFailed')}</p>
      {/if}
    </div>
  </section>
</WorkspaceShell>

<style>
  .drawer-board, .drawer-controls { min-width: 0; }
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
  .page-strip {
    display: grid;
    gap: 8px;
    grid-template-columns: repeat(auto-fill, minmax(96px, 1fr));
    margin-top: 14px;
    max-width: 100%;
    min-width: 0;
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
    .flag-grid { grid-template-columns: 1fr; }
    .export-row { align-items: stretch; display: grid; }
    .copy-document { width: 100%; }
  }
</style>
