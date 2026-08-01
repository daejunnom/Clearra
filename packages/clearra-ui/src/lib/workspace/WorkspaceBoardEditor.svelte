<script lang="ts">
  import {
    ArrowDownToLine,
    FileUp,
    FlipHorizontal2,
    Layers,
    Redo2,
    Target,
    Trash2,
    Undo2,
    Upload
  } from '@lucide/svelte';
  import { createEventDispatcher, onMount } from 'svelte';

  import {
    CTK3_FILE_ACCEPT,
    installGlobalDocumentPaste,
    sourceFromCtk3File
  } from './ctk3File';
  import { decodeInterchangeField } from './fumenFieldImport';
  import {
    boardCellMask,
    boardCellOccupied,
    mirrorBoardMask,
    occupiedCellCount
  } from './solverWorkspaceModel';
  import {
    workspaceMessage,
    type WorkspaceLanguage,
    type WorkspaceMessageKey
  } from './workspaceI18n';

  type BoardEditorMode = 'pc' | 'build-probability' | 'forward';
  type Layer = 'existing' | 'target';
  type Snapshot = { existingMask: bigint; targetMask: bigint };

  export let mode: BoardEditorMode;
  export let height: number;
  export let existingMask: bigint;
  export let targetMask = 0n;
  export let piecesNeeded: number | null;
  export let language: WorkspaceLanguage;
  export let showImport = true;
  export let showStats = true;
  export let showToolbar = true;

  const dispatch = createEventDispatcher<{
    change: Snapshot;
    import: { existingMask: bigint; height: number };
  }>();
  const columns = Array.from({ length: 10 }, (_, index) => index);
  let activeLayer: Layer = mode === 'build-probability' ? 'target' : 'existing';
  let undoStack: Snapshot[] = [];
  let redoStack: Snapshot[] = [];
  let painting = false;
  let paintingPointer: number | null = null;
  let paintOccupied = true;
  let dragExisting = existingMask;
  let dragTarget = targetMask;
  let boardElement: HTMLDivElement | null = null;
  let importInput = '';
  let importError = false;
  let fileInput: HTMLInputElement | null = null;
  let lastExisting = existingMask;
  let lastTarget = targetMask;
  let boardLabel: WorkspaceMessageKey = 'field';

  $: rows = Array.from({ length: height }, (_, index) => height - index - 1);
  $: existingCells = occupiedCellCount(trimMask(existingMask));
  $: targetCells = occupiedCellCount(trimMask(targetMask));
  $: label = (key: Parameters<typeof workspaceMessage>[1]) => workspaceMessage(language, key);
  $: boardLabel =
    mode === 'build-probability'
      ? 'buildField'
      : mode === 'forward'
        ? 'existingField'
        : 'field';
  $: if (existingMask !== lastExisting || targetMask !== lastTarget) {
    undoStack = [];
    redoStack = [];
    dragExisting = existingMask;
    dragTarget = targetMask;
    lastExisting = existingMask;
    lastTarget = targetMask;
  }

  onMount(() => installGlobalDocumentPaste({
    importSource: (source) => importField(source),
    importFailed: () => (importError = true)
  }));

  function beginPaint(event: PointerEvent, x: number, y: number) {
    if (!event.isPrimary || event.button !== 0) return;
    event.preventDefault();
    painting = true;
    paintingPointer = event.pointerId;
    boardElement?.setPointerCapture(event.pointerId);
    const activeMask = activeLayer === 'existing' ? existingMask : targetMask;
    paintOccupied = !boardCellOccupied(activeMask, x, y);
    undoStack = [...undoStack.slice(-63), snapshot()];
    redoStack = [];
    dragExisting = existingMask;
    dragTarget = targetMask;
    paintCell(x, y);
  }

  function continuePaint(event: PointerEvent) {
    if (!painting || event.pointerId !== paintingPointer || !boardElement) return;
    event.preventDefault();
    const bounds = boardElement.getBoundingClientRect();
    if (bounds.width <= 0 || bounds.height <= 0) return;
    const x = Math.floor(((event.clientX - bounds.left) * 10) / bounds.width);
    const rowFromTop = Math.floor(((event.clientY - bounds.top) * height) / bounds.height);
    if (x < 0 || x >= 10 || rowFromTop < 0 || rowFromTop >= height) return;
    paintCell(x, height - rowFromTop - 1);
  }

  function keyboardToggle(event: MouseEvent, x: number, y: number) {
    if (event.detail !== 0) return;
    const activeMask = activeLayer === 'existing' ? existingMask : targetMask;
    commitCell(x, y, !boardCellOccupied(activeMask, x, y));
  }

  function paintCell(x: number, y: number) {
    const next = setLayerCell(dragExisting, dragTarget, x, y, paintOccupied);
    if (next.existingMask === dragExisting && next.targetMask === dragTarget) return;
    dragExisting = next.existingMask;
    dragTarget = next.targetMask;
    emit(next);
  }

  function commitCell(x: number, y: number, occupied: boolean) {
    const next = setLayerCell(existingMask, targetMask, x, y, occupied);
    commit(next);
  }

  function setLayerCell(
    currentExisting: bigint,
    currentTarget: bigint,
    x: number,
    y: number,
    occupied: boolean
  ): Snapshot {
    const cell = boardCellMask(x, y);
    if (activeLayer === 'target' && mode === 'build-probability') {
      return {
        existingMask: occupied ? currentExisting & ~cell : currentExisting,
        targetMask: occupied ? currentTarget | cell : currentTarget & ~cell
      };
    }
    return {
      existingMask: occupied ? currentExisting | cell : currentExisting & ~cell,
      targetMask:
        mode === 'build-probability' && occupied ? currentTarget & ~cell : currentTarget
    };
  }

  function commit(next: Snapshot, recordHistory = true) {
    const normalized = normalize(next);
    if (normalized.existingMask === existingMask && normalized.targetMask === targetMask) return;
    if (recordHistory) {
      undoStack = [...undoStack.slice(-63), snapshot()];
      redoStack = [];
    }
    emit(normalized);
  }

  function emit(next: Snapshot) {
    const normalized = normalize(next);
    lastExisting = normalized.existingMask;
    lastTarget = normalized.targetMask;
    dispatch('change', normalized);
  }

  function snapshot(): Snapshot {
    return { existingMask, targetMask };
  }

  function normalize(next: Snapshot): Snapshot {
    const existing = trimMask(next.existingMask);
    return {
      existingMask: existing,
      targetMask: mode === 'build-probability' ? trimMask(next.targetMask) & ~existing : 0n
    };
  }

  function trimMask(mask: bigint): bigint {
    const cells = Math.max(0, Math.min(240, Math.trunc(height) * 10));
    return cells === 0 ? 0n : mask & ((1n << BigInt(cells)) - 1n);
  }

  function undo() {
    const previous = undoStack.at(-1);
    if (!previous) return;
    redoStack = [...redoStack, snapshot()];
    undoStack = undoStack.slice(0, -1);
    emit(previous);
  }

  function redo() {
    const next = redoStack.at(-1);
    if (!next) return;
    undoStack = [...undoStack, snapshot()];
    redoStack = redoStack.slice(0, -1);
    emit(next);
  }

  function mirror() {
    commit({
      existingMask: mirrorBoardMask(existingMask, height),
      targetMask: mode === 'build-probability' ? mirrorBoardMask(targetMask, height) : 0n
    });
  }

  function clearField() {
    commit({ existingMask: 0n, targetMask: 0n });
  }

  function useClearedBuildAsExisting() {
    if (mode !== 'build-probability' || targetMask === 0n) return;
    commit({
      existingMask: clearCompletedRows(existingMask | targetMask),
      targetMask: 0n
    });
    activeLayer = 'target';
  }

  function clearCompletedRows(board: bigint): bigint {
    const fullRow = (1n << 10n) - 1n;
    let compacted = 0n;
    let outputRow = 0;
    for (let inputRow = 0; inputRow < height; inputRow += 1) {
      const row = (board >> BigInt(inputRow * 10)) & fullRow;
      if (row === fullRow) continue;
      compacted |= row << BigInt(outputRow * 10);
      outputRow += 1;
    }
    return trimMask(compacted);
  }

  function importField(source = importInput) {
    try {
      const imported = decodeInterchangeField(source, mode === 'pc' ? 6 : 24);
      importError = false;
      dispatch('import', {
        existingMask: imported.boardMask,
        height: Math.max(height, imported.occupiedHeight || 1)
      });
    } catch {
      importError = true;
    }
  }

  async function importCtk3File(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file) return;
    try {
      importField(await sourceFromCtk3File(file));
    } catch {
      importError = true;
    }
  }

  function stopPainting(event: PointerEvent) {
    if (paintingPointer !== null && event.pointerId !== paintingPointer) return;
    if (paintingPointer !== null && boardElement?.hasPointerCapture(paintingPointer)) {
      boardElement.releasePointerCapture(paintingPointer);
    }
    painting = false;
    paintingPointer = null;
  }
</script>

<svelte:window on:pointerup={stopPainting} on:pointercancel={stopPainting} />

<section class="board-tool" aria-label={label(boardLabel)}>
  {#if showToolbar}
    <div class="section-heading">
      <div>
        <span class="eyebrow">{label(boardLabel)}</span>
        <strong>{height}L · 10×{height}</strong>
      </div>
      <div class="board-actions" role="toolbar" aria-label={label(boardLabel)}>
        <button type="button" title={label('undo')} aria-label={label('undo')} disabled={!undoStack.length} on:click={undo}>
          <Undo2 size={16} strokeWidth={1.8} />
        </button>
        <button type="button" title={label('redo')} aria-label={label('redo')} disabled={!redoStack.length} on:click={redo}>
          <Redo2 size={16} strokeWidth={1.8} />
        </button>
        {#if mode === 'build-probability'}
          <span class="toolbar-divider" aria-hidden="true"></span>
          <button
            type="button"
            class="layer-button existing-layer"
            class:active={activeLayer === 'existing'}
            title={label('editExistingLayer')}
            aria-label={label('editExistingLayer')}
            aria-pressed={activeLayer === 'existing'}
            on:click={() => (activeLayer = 'existing')}
          ><Layers size={16} strokeWidth={1.8} /></button>
          <button
            type="button"
            class="layer-button target-layer"
            class:active={activeLayer === 'target'}
            title={label('editTargetLayer')}
            aria-label={label('editTargetLayer')}
            aria-pressed={activeLayer === 'target'}
            on:click={() => (activeLayer = 'target')}
          ><Target size={16} strokeWidth={1.8} /></button>
          <span class="toolbar-divider" aria-hidden="true"></span>
        {/if}
        <button type="button" title={label('mirrorField')} aria-label={label('mirrorField')} on:click={mirror}>
          <FlipHorizontal2 size={16} strokeWidth={1.8} />
        </button>
        <button
          type="button"
          title={label('clearField')}
          aria-label={label('clearField')}
          disabled={existingMask === 0n && targetMask === 0n}
          on:click={clearField}
        ><Trash2 size={16} strokeWidth={1.8} /></button>
      </div>
    </div>
  {/if}

  {#if showImport}
    <div class="fumen-import">
      <label>
        <span>{label(mode === 'pc' ? 'fieldImport' : 'existingFieldImport')}</span>
        <input
          value={importInput}
          placeholder="v115@... / ctk3_..."
          spellcheck="false"
          aria-invalid={importError}
          on:input={(event) => {
            importInput = (event.currentTarget as HTMLInputElement).value;
            importError = false;
          }}
          on:keydown={(event) => event.key === 'Enter' && importField()}
        />
      </label>
      <input
        bind:this={fileInput}
        class="file-input"
        type="file"
        accept={CTK3_FILE_ACCEPT}
        hidden
        on:change={importCtk3File}
      />
      <button type="button" title={label('loadCtk3File')} on:click={() => fileInput?.click()}>
        <FileUp size={15} strokeWidth={1.8} />{label('loadCtk3File')}
      </button>
      <button type="button" disabled={!importInput.trim()} on:click={() => importField()}>
        <Upload size={15} strokeWidth={1.8} />{label('loadField')}
      </button>
    </div>
    {#if importError}<p class="fumen-error" role="alert">{label('fieldImportInvalid')}</p>{/if}
  {/if}

  {#if mode === 'build-probability'}
    <div class="layer-help">
      <div class="legend" aria-label={label('editLayer')}>
        <span><i class="existing"></i>{label('existingField')}</span>
        <span><i class="target"></i>{label('targetBuild')}</span>
      </div>
      <p>{label('editLayerHelp')}</p>
    </div>
  {/if}

  <div class="board-frame">
    <div
      bind:this={boardElement}
      class:pc={mode !== 'build-probability'}
      class:build={mode === 'build-probability'}
      class="board"
      role="group"
      aria-label={label(boardLabel)}
      style={`--board-rows:${height}`}
      on:pointermove={continuePaint}
    >
      {#each rows as y}
        {#each columns as x}
          <button
            type="button"
            class:existing={boardCellOccupied(existingMask, x, y)}
            class:target={mode === 'build-probability' && boardCellOccupied(targetMask, x, y)}
            aria-label={`${label(mode === 'pc' ? 'field' : mode === 'forward' || activeLayer === 'existing' ? 'existingField' : 'targetBuild')} ${x + 1}, ${y + 1}`}
            aria-pressed={boardCellOccupied(activeLayer === 'existing' ? existingMask : targetMask, x, y)}
            on:pointerdown={(event) => beginPaint(event, x, y)}
            on:click={(event) => keyboardToggle(event, x, y)}
          ><span></span></button>
        {/each}
      {/each}
    </div>
  </div>

  {#if showStats}
    <dl class:build-stats={mode === 'build-probability'} class="board-stats">
      {#if mode !== 'build-probability'}
        <div><dt>{label('filledCells')}</dt><dd>{existingCells}</dd></div>
      {:else}
        <div><dt>{label('existingCells')}</dt><dd>{existingCells}</dd></div>
        <div><dt>{label('targetCells')}</dt><dd>{targetCells}</dd></div>
      {/if}
      <div><dt>{label('piecesNeeded')}</dt><dd>{piecesNeeded ?? '—'}</dd></div>
    </dl>
  {/if}
  {#if mode === 'build-probability'}
    <button
      class="continue-button"
      type="button"
      disabled={targetMask === 0n}
      on:click={useClearedBuildAsExisting}
    >
      <ArrowDownToLine size={15} strokeWidth={1.8} />{label('useAsNextBase')}
    </button>
  {/if}
</section>

<style>
  .board-tool { min-width: 0; }
  .section-heading { align-items: flex-end; display: flex; gap: 16px; justify-content: space-between; margin-bottom: 14px; }
  .section-heading > div:first-child { display: grid; gap: 3px; }
  .eyebrow { color: #66716d; font-size: 11px; font-weight: 700; text-transform: uppercase; }
  strong { color: #17211e; font-size: 15px; }
  .board-actions { align-items: center; display: flex; gap: 5px; }
  .board-actions button { align-items: center; background: #fff; border: 1px solid #cbd3ce; border-radius: 5px; color: #34403c; cursor: pointer; display: inline-flex; height: 32px; justify-content: center; padding: 0; width: 32px; }
  .board-actions button:hover:not(:disabled), .board-actions button.active { background: #e4f1ee; border-color: #36847c; color: #075f58; }
  .board-actions button:disabled { cursor: default; opacity: .35; }
  .board-actions .layer-button { position: relative; }
  .board-actions .layer-button.active::after { background: #16877d; bottom: 2px; content: ''; height: 2px; left: 6px; position: absolute; right: 6px; }
  .toolbar-divider { background: #d8dfdb; height: 22px; margin: 0 2px; width: 1px; }
  .fumen-import { align-items: end; display: grid; gap: 8px; grid-template-columns: minmax(0, 1fr) auto auto; margin-bottom: 12px; }
  .fumen-import label { display: grid; gap: 6px; }
  .fumen-import label span { color: #68736f; font-size: 11px; font-weight: 650; }
  .fumen-import input { border: 1px solid #cbd3ce; border-radius: 5px; color: #17211e; font-family: ui-monospace, SFMono-Regular, Consolas, monospace; font-size: 12px; height: 36px; min-width: 0; padding: 0 9px; width: 100%; }
  .fumen-import input[aria-invalid='true'] { border-color: #c45635; }
  .fumen-import .file-input { display: none; }
  .fumen-import button { align-items: center; background: #fff; border: 1px solid #aebbb5; border-radius: 5px; color: #27403a; cursor: pointer; display: inline-flex; font-size: 12px; font-weight: 700; gap: 6px; height: 36px; padding: 0 12px; }
  .fumen-import button:disabled { cursor: default; opacity: .4; }
  .fumen-error { color: #9b3c22; font-size: 11px; margin: -5px 0 10px; }
  .layer-help { align-items: center; display: flex; gap: 12px; justify-content: space-between; margin: -2px 0 8px; }
  .layer-help p { color: #6a7570; font-size: 10px; margin: 0; text-align: right; }
  .legend { display: flex; flex: 0 0 auto; gap: 15px; }
  .legend span { align-items: center; color: #65716c; display: inline-flex; font-size: 11px; gap: 6px; }
  .legend i { border: 1px solid #4d5a56; display: block; height: 11px; width: 11px; }
  .legend i.existing { background: #737d79; }
  .legend i.target { background: #d8e2de; }
  .board-frame { background: #101817; border: 1px solid #253330; border-radius: 6px; box-shadow: inset 0 0 0 1px #090d0c; margin: 0 auto; max-width: 520px; padding: 14px; }
  .board { -webkit-user-select: none; aspect-ratio: calc(10 / var(--board-rows)); display: grid; gap: 0; grid-template-columns: repeat(10, minmax(0, 1fr)); grid-template-rows: repeat(var(--board-rows), minmax(0, 1fr)); margin: 0 auto; max-height: 500px; max-width: 100%; touch-action: none; user-select: none; }
  .board > button { aspect-ratio: 1; background: #1e2927; border: 0; border-radius: 0; box-shadow: inset 0 0 0 1px rgba(216, 226, 222, .2); cursor: crosshair; min-width: 0; padding: 0; }
  .board > button:hover, .board > button:focus-visible { background: #33423f; outline: 2px solid #75c8bc; outline-offset: -2px; }
  .board.build > button.existing { background: #737d79; box-shadow: inset 2px 2px 0 rgba(255,255,255,.1), inset -2px -2px 0 rgba(20,26,24,.25); }
  .board.pc > button.existing, .board > button.target { background: #d8e2de; box-shadow: inset 2px 2px 0 rgba(255,255,255,.16), inset -2px -2px 0 rgba(41,56,51,.18); }
  .board > button span { display: block; height: 100%; width: 100%; }
  .board-stats { display: grid; gap: 1px; grid-template-columns: repeat(2, minmax(0, 1fr)); margin: 10px 0 0; }
  .board-stats.build-stats { grid-template-columns: repeat(3, minmax(0, 1fr)); }
  .board-stats div { align-items: baseline; background: #eef2ef; display: flex; justify-content: space-between; min-width: 0; padding: 9px 11px; }
  .board-stats div:first-child { border-radius: 5px 0 0 5px; }
  .board-stats div:last-child { border-radius: 0 5px 5px 0; }
  .continue-button { align-items: center; background: #fff; border: 1px solid #aebbb5; border-radius: 5px; color: #27403a; cursor: pointer; display: inline-flex; font-size: 11px; font-weight: 700; gap: 7px; margin-top: 10px; min-height: 34px; padding: 7px 10px; }
  .continue-button:disabled { cursor: default; opacity: .4; }
  dt { color: #66716d; font-size: 12px; }
  dd { color: #17211e; font-size: 14px; font-weight: 750; margin: 0; }
  @media (max-width: 620px) {
    .section-heading { align-items: stretch; flex-direction: column; }
    .board-actions { justify-content: flex-start; }
    .board-frame { padding: 9px; }
    .fumen-import { grid-template-columns: 1fr; }
    .fumen-import button { justify-content: center; width: 100%; }
    .layer-help { align-items: flex-start; flex-direction: column; }
    .layer-help p { text-align: left; }
  }
</style>
