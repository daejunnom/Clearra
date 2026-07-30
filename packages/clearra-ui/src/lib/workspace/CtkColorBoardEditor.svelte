<script lang="ts">
  import {
    ArrowDownToLine,
    CircleOff,
    DropletOff,
    Eraser,
    FlipHorizontal2,
    Layers2,
    SquareX
  } from '@lucide/svelte';
  import { createEventDispatcher } from 'svelte';

  import type {
    Ctk3Color,
    Ctk3Operation,
    Ctk3Piece,
    Ctk3Rotation
  } from './ctk3Codec';
  import {
    isOperationPlaceable,
    operationCells
  } from './ctkPageTools';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';

  export let height = 8;
  export let cells: Ctk3Color[] = [];
  export let operation: Ctk3Operation | undefined = undefined;
  export let language: WorkspaceLanguage;

  const dispatch = createEventDispatcher<{
    change: Ctk3Color[];
    mirror: void;
    lineclear: { grayscale: boolean };
    grayscale: void;
    clear: void;
    operation: Ctk3Operation | undefined;
  }>();
  const palette: Array<Exclude<Ctk3Color, null>> = [
    'G',
    'I',
    'O',
    'T',
    'S',
    'Z',
    'J',
    'L'
  ];
  const pieces: Ctk3Piece[] = ['I', 'O', 'T', 'S', 'Z', 'J', 'L'];
  const rotations: Ctk3Rotation[] = ['spawn', 'right', 'reverse', 'left'];
  const rotationSymbols: Record<Ctk3Rotation, string> = {
    spawn: '0',
    right: 'R',
    reverse: '2',
    left: 'L'
  };
  let selected: Ctk3Color = 'G';
  let board: HTMLDivElement | null = null;
  let activePointer: number | null = null;
  let workingCells = cells;
  let lastCell = -1;
  let dragColor: Ctk3Color = 'G';
  let paintMode = true;
  let placingOperation = false;
  let localOperation = operation ? { ...operation } : undefined;
  let operationPiece: Ctk3Piece | null = operation?.piece ?? null;
  let operationRotation: Ctk3Rotation = operation?.rotation ?? 'spawn';
  let observedExternalOperationKey = operationKey(operation);
  let hoverAnchor: { x: number; y: number } | null = null;

  $: rows = Array.from({ length: height }, (_, index) => height - index - 1);
  $: normalizedCells = normalizeCells(cells, height);
  $: availableRotations = operationPiece
    ? rotationsForPiece(operationPiece)
    : [];
  $: if (activePointer === null) workingCells = normalizedCells;
  $: externalOperationKey = operationKey(operation);
  $: if (externalOperationKey !== observedExternalOperationKey) {
    observedExternalOperationKey = externalOperationKey;
    localOperation = operation ? { ...operation } : undefined;
    operationPiece = operation?.piece ?? null;
    operationRotation = operation
      ? canonicalRotation(operation.piece, operation.rotation)
      : operationRotation;
    placingOperation = false;
    hoverAnchor = null;
  }
  $: previewOperation = placingOperation && operationPiece && hoverAnchor
    ? {
        piece: operationPiece,
        rotation: operationRotation,
        x: hoverAnchor.x,
        y: hoverAnchor.y
      } satisfies Ctk3Operation
    : localOperation;
  $: previewValid = previewOperation
    ? isOperationPlaceable(
        previewOperation,
        normalizedCells,
        10,
        height
      )
    : false;
  $: previewIndexes = new Set(
    previewOperation
      ? operationCells(previewOperation)
          .filter(({ x, y }) => x >= 0 && x < 10 && y >= 0 && y < height)
          .map(({ x, y }) => y * 10 + x)
      : []
  );
  $: axisIndex =
    placingOperation && hoverAnchor
      ? hoverAnchor.y * 10 + hoverAnchor.x
      : -1;
  $: label = (key: Parameters<typeof workspaceMessage>[1]) =>
    workspaceMessage(language, key);

  function beginPaint(event: PointerEvent, x: number, y: number) {
    if (!event.isPrimary || event.button !== 0) return;
    event.preventDefault();
    if (placingOperation) {
      placeOperation(x, y);
      return;
    }
    if (!paintMode) return;
    activePointer = event.pointerId;
    lastCell = -1;
    workingCells = normalizedCells.slice();
    dragColor =
      workingCells[y * 10 + x] === null
        ? selected
        : null;
    board?.setPointerCapture(event.pointerId);
    paint(x, y);
  }

  function handlePointerMove(event: PointerEvent) {
    const cell = pointerCell(event);
    if (placingOperation) {
      hoverAnchor = cell;
      return;
    }
    if (event.pointerId !== activePointer || !cell) return;
    paint(cell.x, cell.y);
  }

  function pointerCell(event: PointerEvent): { x: number; y: number } | null {
    if (!board) return null;
    const bounds = board.getBoundingClientRect();
    if (bounds.width <= 0 || bounds.height <= 0) return null;
    const x = Math.floor(((event.clientX - bounds.left) / bounds.width) * 10);
    const displayY = Math.floor(
      ((event.clientY - bounds.top) / bounds.height) * height
    );
    if (x < 0 || x >= 10 || displayY < 0 || displayY >= height) return null;
    return { x, y: height - displayY - 1 };
  }

  function finishPaint(event: PointerEvent) {
    if (activePointer === null || event.pointerId !== activePointer) return;
    if (board?.hasPointerCapture(activePointer)) {
      board.releasePointerCapture(activePointer);
    }
    activePointer = null;
    lastCell = -1;
  }

  function keyboardToggle(event: MouseEvent, x: number, y: number) {
    if (event.detail !== 0) return;
    if (placingOperation) {
      placeOperation(x, y);
      return;
    }
    if (!paintMode) return;
    const index = y * 10 + x;
    const next = normalizedCells.slice();
    next[index] = next[index] === null ? selected : null;
    dispatch('change', next);
  }

  function paint(x: number, y: number) {
    const index = y * 10 + x;
    if (index === lastCell || workingCells[index] === dragColor) return;
    lastCell = index;
    workingCells[index] = dragColor;
    workingCells = workingCells.slice();
    dispatch('change', workingCells);
  }

  function activatePaint(color: Ctk3Color) {
    selected = color;
    paintMode = true;
    placingOperation = false;
    hoverAnchor = null;
  }

  function chooseOperationPiece(piece: Ctk3Piece | null) {
    paintMode = false;
    if (!piece) {
      operationPiece = null;
      placingOperation = false;
      hoverAnchor = null;
      localOperation = undefined;
      dispatch('operation', undefined);
      return;
    }
    commitStoredOperation();
    operationPiece = piece;
    operationRotation = canonicalRotation(piece, operationRotation);
    placingOperation = true;
    hoverAnchor = null;
  }

  function chooseOperationRotation(rotation: Ctk3Rotation) {
    if (!operationPiece) return;
    paintMode = false;
    placingOperation = true;
    operationRotation = rotation;
    hoverAnchor = null;
  }

  function placeOperation(x: number, y: number) {
    if (!operationPiece) return;
    const candidate: Ctk3Operation = {
      piece: operationPiece,
      rotation: operationRotation,
      x,
      y
    };
    if (!isOperationPlaceable(candidate, normalizedCells, 10, height)) return;
    const next = normalizedCells.slice();
    for (const { x: cellX, y: cellY } of operationCells(candidate)) {
      next[cellY * 10 + cellX] = candidate.piece;
    }
    workingCells = next;
    paintMode = false;
    hoverAnchor = null;
    dispatch('change', next);
  }

  function commitStoredOperation() {
    const stored = localOperation;
    if (!stored) return;
    if (isOperationPlaceable(stored, normalizedCells, 10, height)) {
      const next = normalizedCells.slice();
      for (const { x, y } of operationCells(stored)) {
        next[y * 10 + x] = stored.piece;
      }
      workingCells = next;
      dispatch('change', next);
    }
    localOperation = undefined;
    dispatch('operation', undefined);
  }

  function operationKey(value: Ctk3Operation | undefined): string {
    return value
      ? `${value.piece}:${value.rotation}:${value.x}:${value.y}`
      : '';
  }

  function rotationsForPiece(piece: Ctk3Piece): Ctk3Rotation[] {
    if (piece === 'O') return [];
    if (piece === 'I' || piece === 'S' || piece === 'Z') {
      return ['spawn', 'right'];
    }
    return rotations;
  }

  function canonicalRotation(
    piece: Ctk3Piece,
    rotation: Ctk3Rotation
  ): Ctk3Rotation {
    if (piece === 'O') return 'spawn';
    if (piece === 'I' || piece === 'S' || piece === 'Z') {
      return rotation === 'right' || rotation === 'left' ? 'right' : 'spawn';
    }
    return rotation;
  }

  function normalizeCells(source: Ctk3Color[], rows: number): Ctk3Color[] {
    const result = Array<Ctk3Color>(rows * 10).fill(null);
    result.splice(
      0,
      Math.min(result.length, source.length),
      ...source.slice(0, result.length)
    );
    return result;
  }
</script>

<svelte:window on:pointerup={finishPaint} on:pointercancel={finishPaint} />

<div class="palette" role="toolbar" aria-label={label('ctkPalette')}>
  {#each palette as color}
    <button
      type="button"
      class:active={selected === color && paintMode}
      class={`swatch piece-${color}`}
      title={color === 'G' ? label('ctkFieldColor') : color}
      aria-label={color === 'G' ? label('ctkFieldColor') : color}
      aria-pressed={selected === color && paintMode}
      on:click={() => activatePaint(color)}
    ><span>{color === 'G' ? '' : color}</span></button>
  {/each}
  <span class="palette-divider" aria-hidden="true"></span>
  <button
    type="button"
    class="tool-button"
    title={label('mirrorField')}
    aria-label={label('mirrorField')}
    on:click={() => dispatch('mirror')}
  ><FlipHorizontal2 size={16} strokeWidth={1.8} /></button>
  <button
    type="button"
    class="tool-button"
    title={label('ctkClearLines')}
    aria-label={label('ctkClearLines')}
    on:click={() => dispatch('lineclear', { grayscale: false })}
  ><ArrowDownToLine size={16} strokeWidth={1.8} /></button>
  <button
    type="button"
    class="tool-button"
    title={label('ctkRemoveColors')}
    aria-label={label('ctkRemoveColors')}
    on:click={() => dispatch('grayscale')}
  ><DropletOff size={16} strokeWidth={1.8} /></button>
  <button
    type="button"
    class="tool-button"
    title={label('ctkClearLinesAndColors')}
    aria-label={label('ctkClearLinesAndColors')}
    on:click={() => dispatch('lineclear', { grayscale: true })}
  ><Layers2 size={16} strokeWidth={1.8} /></button>
  <button
    type="button"
    class="tool-button"
    title={label('clearField')}
    aria-label={label('clearField')}
    on:click={() => dispatch('clear')}
  ><SquareX size={16} strokeWidth={1.8} /></button>
  <button
    type="button"
    class="swatch eraser"
    class:active={selected === null && paintMode}
    title={label('ctkEraser')}
    aria-label={label('ctkEraser')}
    aria-pressed={selected === null && paintMode}
    on:click={() => activatePaint(null)}
  ><Eraser size={16} strokeWidth={1.8} /></button>
</div>

<div class="operation-tools">
  <div class="operation-control">
    <span class="control-label">{label('ctkOperation')}</span>
    <div class="piece-options" role="group" aria-label={label('ctkOperation')}>
      <button
        type="button"
        class="operation-none"
        class:active={operationPiece === null && localOperation === undefined}
        title={label('ctkOperationNone')}
        aria-label={label('ctkOperationNone')}
        aria-pressed={operationPiece === null && localOperation === undefined}
        on:click={() => chooseOperationPiece(null)}
      ><CircleOff size={16} strokeWidth={1.8} /></button>
      {#each pieces as piece}
        <button
          type="button"
          class={`operation-piece piece-${piece}`}
          class:active={operationPiece === piece}
          title={piece}
          aria-label={piece}
          aria-pressed={operationPiece === piece}
          on:click={() => chooseOperationPiece(piece)}
        >{piece}</button>
      {/each}
    </div>
  </div>
  <div class="operation-control rotation-control">
    <span class="control-label">{label('rotation')}</span>
    <div class="rotation-options" role="group" aria-label={label('rotation')}>
      {#each rotations as rotation}
        <button
          type="button"
          class:active={operationRotation === rotation && operationPiece !== null}
          class:unavailable={!availableRotations.includes(rotation)}
          disabled={operationPiece === null || !availableRotations.includes(rotation)}
          title={label(`ctkRotation${rotation[0].toUpperCase()}${rotation.slice(1)}` as Parameters<typeof workspaceMessage>[1])}
          aria-label={label(`ctkRotation${rotation[0].toUpperCase()}${rotation.slice(1)}` as Parameters<typeof workspaceMessage>[1])}
          aria-pressed={operationRotation === rotation && operationPiece !== null}
          on:click={() => chooseOperationRotation(rotation)}
        >{rotationSymbols[rotation]}</button>
      {/each}
    </div>
  </div>
  <span class="operation-hint">{label('ctkOperationPlacementHelp')}</span>
</div>

<div class="board-frame">
  <div
    bind:this={board}
    class="board"
    class:operation-placement={placingOperation}
    class:operation-idle={!paintMode && !placingOperation}
    style={`--rows:${height};aspect-ratio:${10 / height}`}
    role="grid"
    tabindex="0"
    aria-label={label('ctkDrawerBoard')}
    on:pointermove={handlePointerMove}
    on:pointerleave={() => (hoverAnchor = null)}
  >
    {#each rows as y}
      {#each Array.from({ length: 10 }, (_, index) => index) as x}
        {@const index = y * 10 + x}
        {@const color = workingCells[index] ?? null}
        <button
          type="button"
          class:empty={color === null}
          class:field={color === 'G'}
          class:operation-preview={previewIndexes.has(index)}
          class:operation-invalid={previewIndexes.has(index) && !previewValid}
          class:operation-placed={previewIndexes.has(index) && localOperation !== undefined && !placingOperation}
          class:operation-axis={index === axisIndex}
          class={`cell piece-${color ?? 'empty'} operation-piece-${previewOperation?.piece ?? 'none'}`}
          aria-label={`${x + 1}, ${y + 1}`}
          on:pointerdown={(event) => beginPaint(event, x, y)}
          on:click={(event) => keyboardToggle(event, x, y)}
        ></button>
      {/each}
    {/each}
  </div>
</div>

<style>
  .palette {
    align-items: center;
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
    margin-bottom: 10px;
  }

  .swatch, .tool-button {
    align-items: center;
    border-radius: 5px;
    color: #17211e;
    cursor: pointer;
    display: inline-flex;
    font: inherit;
    font-size: 10px;
    font-weight: 800;
    height: 34px;
    justify-content: center;
    padding: 0;
    width: 34px;
  }

  .swatch {
    border: 2px solid transparent;
    box-shadow: inset 2px 2px 0 rgba(255, 255, 255, .15), inset -2px -2px 0 rgba(0, 0, 0, .16);
  }

  .swatch.active {
    border-color: #075f58;
    box-shadow: 0 0 0 2px #fff, 0 0 0 4px #16877d;
  }

  .tool-button {
    background: #fff;
    border: 1px solid #bfc9c4;
    color: #485651;
  }

  .tool-button:hover {
    background: #e4f1ee;
    border-color: #36847c;
    color: #075f58;
  }

  .palette-divider {
    background: #d5ddd8;
    height: 24px;
    margin: 0 2px;
    width: 1px;
  }

  .eraser {
    background: #fff;
    border: 1px solid #bfc9c4;
    box-shadow: none;
    color: #485651;
  }

  .operation-tools {
    align-items: end;
    display: grid;
    gap: 8px;
    grid-template-columns: minmax(270px, auto) minmax(150px, auto) minmax(0, 1fr);
    margin-bottom: 12px;
  }

  .operation-control {
    display: grid;
    gap: 4px;
  }

  .control-label {
    color: #68736f;
    font-size: 10px;
    font-weight: 700;
  }

  .piece-options,
  .rotation-options {
    align-items: center;
    display: flex;
    gap: 4px;
  }

  .piece-options button,
  .rotation-options button {
    align-items: center;
    background: #fff;
    border: 1px solid #cbd3ce;
    border-radius: 5px;
    color: #26322e;
    cursor: pointer;
    display: inline-flex;
    font: inherit;
    font-size: 10px;
    font-weight: 800;
    height: 34px;
    justify-content: center;
    padding: 0;
    width: 34px;
  }

  .piece-options button.active,
  .rotation-options button.active {
    border-color: #075f58;
    box-shadow: 0 0 0 1px #16877d;
  }

  .operation-piece {
    box-shadow: inset 2px 2px 0 rgba(255, 255, 255, .15), inset -2px -2px 0 rgba(0, 0, 0, .16);
  }

  .operation-none {
    color: #63706b;
  }

  .rotation-options button {
    width: 36px;
  }

  .rotation-options button:disabled {
    cursor: default;
    opacity: .45;
  }

  .rotation-options button.unavailable {
    visibility: hidden;
  }

  .operation-hint {
    color: #6c7773;
    font-size: 10px;
    line-height: 1.4;
    padding-bottom: 3px;
  }

  .board-frame {
    background: #101817;
    border: 1px solid #253330;
    border-radius: 6px;
    padding: 14px;
  }

  .board {
    display: grid;
    grid-template-columns: repeat(10, minmax(0, 1fr));
    grid-template-rows: repeat(var(--rows), minmax(0, 1fr));
    margin: 0 auto;
    max-height: 640px;
    max-width: 100%;
    touch-action: none;
    user-select: none;
  }

  .cell {
    border: 0;
    border-radius: 0;
    cursor: crosshair;
    min-height: 0;
    min-width: 0;
    padding: 0;
    position: relative;
  }

  .board.operation-idle .cell {
    cursor: default;
  }

  .board.operation-placement .cell {
    cursor: crosshair;
  }

  .cell.empty {
    background: #1e2927;
    box-shadow: inset 0 0 0 1px rgba(216, 226, 222, .2);
  }

  .cell:not(.empty) {
    box-shadow: inset 2px 2px 0 rgba(255,255,255,.14), inset -2px -2px 0 rgba(20,26,24,.22);
  }

  .cell.operation-preview::after {
    background: var(--operation-color);
    box-shadow: inset 2px 2px 0 rgba(255,255,255,.2), inset -2px -2px 0 rgba(20,26,24,.2);
    content: '';
    inset: 1px;
    opacity: .78;
    pointer-events: none;
    position: absolute;
  }

  .cell.operation-placed::after {
    opacity: 1;
  }

  .cell.operation-invalid::after {
    background: rgba(198, 77, 58, .65);
    box-shadow: inset 0 0 0 2px #ffd4cc;
  }

  .cell.operation-axis::before {
    background: #fff;
    border: 1px solid #123a35;
    border-radius: 50%;
    content: '';
    height: 6px;
    left: 50%;
    pointer-events: none;
    position: absolute;
    top: 50%;
    transform: translate(-50%, -50%);
    width: 6px;
    z-index: 2;
  }

  .piece-G { background: #7b8581; }
  .piece-I { background: #55cbd3; }
  .piece-O { background: #f3cf4d; }
  .piece-T { background: #b66ad0; }
  .piece-S { background: #65c778; }
  .piece-Z { background: #e96e6e; }
  .piece-J { background: #628ae0; }
  .piece-L { background: #ef9c4d; }

  .operation-piece-I { --operation-color: #55cbd3; }
  .operation-piece-O { --operation-color: #f3cf4d; }
  .operation-piece-T { --operation-color: #b66ad0; }
  .operation-piece-S { --operation-color: #65c778; }
  .operation-piece-Z { --operation-color: #e96e6e; }
  .operation-piece-J { --operation-color: #628ae0; }
  .operation-piece-L { --operation-color: #ef9c4d; }

  @media (max-width: 620px) {
    .operation-tools {
      grid-template-columns: 1fr;
    }

    .operation-hint {
      grid-column: auto;
    }

    .piece-options {
      flex-wrap: wrap;
    }
  }

  @media (max-width: 560px) {
    .board-frame {
      margin-left: -8px;
      margin-right: -8px;
      padding: 9px;
    }
  }
</style>
