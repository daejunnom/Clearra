<script lang="ts">
  import { createEventDispatcher, onDestroy } from 'svelte';
  import { componentMessage } from '../i18n/componentCatalog';
  import { encodeCtk3Compact, type Ctk3Page } from './ctk3Codec';
  import { openFieldDocument, type FieldDocumentReader } from './fieldInterchange';
  import type { BuildV2Request } from './buildV2Model';
  import type { WorkspaceLanguage } from './workspaceI18n';

  export let request: BuildV2Request;
  export let language: WorkspaceLanguage = 'en';

  const dispatch = createEventDispatcher<{ change: Partial<BuildV2Request> }>();
  let reader: FieldDocumentReader | null = null;
  let loadedSource = '';
  let pageIndex = 0;
  let page: Ctk3Page | null = null;
  let selected = new Map<number, Ctk3Page>();
  let error = '';
  let generation = 0;
  $: source = [
    request.solutionFormat, request.solutionDocument,
    request.capability, request.baseMask.toString(16), request.targetMask.toString(16),
    request.height, request.queue, request.rule, request.queueKnowledge,
    request.holdEnabled, request.holdPiece
  ].join(':');
  $: if (loadedSource && source !== loadedSource) reset();
  $: selectedCount = selected.size;
  $: pageCount = reader?.pageCount ?? 0;
  $: previewRows = page ? rows(page) : [];

  function reset() {
    generation += 1;
    reader?.close();
    reader = null;
    loadedSource = '';
    page = null;
    pageIndex = 0;
    selected = new Map();
    error = '';
    dispatch('change', { pinSolutionFormat: 'ctk3', pinSolutionDocument: '' });
  }

  async function load() {
    reset();
    const currentGeneration = generation;
    try {
      const opened = openFieldDocument(request.solutionDocument);
      if (opened.width !== 10 || opened.pageCount < 1) {
        opened.close();
        throw new Error('The supplied document must contain ten-column solution pages.');
      }
      reader = opened;
      loadedSource = source;
      const first = await opened.readPage(0);
      if (generation !== currentGeneration || reader !== opened) return;
      page = first;
      pageIndex = 0;
    } catch (cause) {
      if (generation === currentGeneration) error = String(cause);
    }
  }

  async function show(index: number) {
    const opened = reader;
    if (!opened || index < 0 || index >= opened.pageCount) return;
    const currentGeneration = generation;
    try {
      const next = await opened.readPage(index);
      if (generation !== currentGeneration || reader !== opened) return;
      page = next;
      pageIndex = index;
      error = '';
    } catch (cause) {
      if (generation === currentGeneration) error = String(cause);
    }
  }

  function toggle() {
    if (!page || !reader) return;
    const next = new Map(selected);
    if (next.has(pageIndex)) next.delete(pageIndex);
    else next.set(pageIndex, page);
    try {
      const pages = [...next.values()];
      const document = pages.length
        ? encodeCtk3Compact({ width: reader.width, pages })
        : '';
      selected = next;
      error = '';
      dispatch('change', { pinSolutionFormat: 'ctk3', pinSolutionDocument: document });
    } catch (cause) {
      error = String(cause);
    }
  }

  function rows(value: Ctk3Page): (string | null)[][] {
    const result: (string | null)[][] = [];
    for (let y = Math.max(value.height, 1) - 1; y >= 0; y -= 1) {
      result.push(value.cells.slice(y * 10, y * 10 + 10));
    }
    return result;
  }

  onDestroy(() => {
    generation += 1;
    reader?.close();
  });
</script>

<section class="pin-selector" aria-label={componentMessage(language, 'selectMandatorySolutions')}>
  <strong>{componentMessage(language, 'selectMandatorySolutions')}</strong>
  <button type="button" on:click={load}>{componentMessage(language, 'loadSuppliedSolutions')}</button>
  {#if reader && page}
    <div class="navigation">
      <button type="button" disabled={pageIndex === 0} on:click={() => show(pageIndex - 1)}>←</button>
      <span>{componentMessage(language, 'solution')} {pageIndex + 1} / {pageCount}</span>
      <button type="button" disabled={pageIndex + 1 >= pageCount} on:click={() => show(pageIndex + 1)}>→</button>
    </div>
    <div class="board" aria-hidden="true">
      {#each previewRows as row}
        {#each row as cell}
          <span class:filled={cell !== null} data-piece={cell ?? ''}></span>
        {/each}
      {/each}
    </div>
    <label class="selection">
      <input type="checkbox" checked={selected.has(pageIndex)} on:change={toggle} />
      {componentMessage(language, 'mandatorySolutions')}
    </label>
    <small>{componentMessage(language, 'selectedSolutions')}: {selectedCount}</small>
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</section>

<style>
  .pin-selector { border: 1px solid #dce3df; border-radius: 6px; display: grid; gap: 10px; padding: 12px; }
  strong { font-size: 12px; }
  button { background: #fff; border: 1px solid #cbd3ce; border-radius: 5px; cursor: pointer; min-height: 32px; padding: 4px 10px; }
  button:disabled { cursor: default; opacity: .5; }
  .navigation, .selection { align-items: center; display: flex; gap: 10px; }
  .navigation { justify-content: space-between; }
  .selection input { height: 16px; width: 16px; }
  .board { display: grid; gap: 1px; grid-template-columns: repeat(10, 13px); width: max-content; }
  .board span { background: #f2f4f2; height: 13px; width: 13px; }
  .board .filled { background: #8b9791; }
  .board [data-piece='I'] { background: #40bbcf; }
  .board [data-piece='O'] { background: #e6cc50; }
  .board [data-piece='T'] { background: #ac76c8; }
  .board [data-piece='S'] { background: #69b966; }
  .board [data-piece='Z'] { background: #d76568; }
  .board [data-piece='J'] { background: #6c83c8; }
  .board [data-piece='L'] { background: #dba052; }
  small { color: #65716c; }
  .error { color: #8b2820; font-size: 11px; margin: 0; overflow-wrap: anywhere; }
</style>
