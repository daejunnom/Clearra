<script lang="ts">
  import type { Ctk3Piece } from '../ctk3Codec';
  import { CTK_COLOR_HEX } from '../ctkBoardTheme';
  import { operationOffsets } from '../ctkOperationGeometry';

  export let piece: Ctk3Piece | null = null;
  export let label = '';
  export let compact = false;
  export let muted = false;

  $: occupied = previewCells(piece);
  $: pieceColor = piece ? CTK_COLOR_HEX[piece] : 'transparent';

  function previewCells(value: Ctk3Piece | null): Set<number> {
    if (!value) return new Set();
    const offsets = operationOffsets(value, 'spawn');
    const minimumX = Math.min(...offsets.map(([x]) => x));
    const maximumX = Math.max(...offsets.map(([x]) => x));
    const minimumY = Math.min(...offsets.map(([, y]) => y));
    const maximumY = Math.max(...offsets.map(([, y]) => y));
    const width = maximumX - minimumX + 1;
    const height = maximumY - minimumY + 1;
    const startX = Math.floor((4 - width) / 2);
    const startY = Math.floor((2 - height) / 2);
    return new Set(
      offsets.map(([x, y]) => {
        const previewX = startX + x - minimumX;
        const previewY = startY + maximumY - y;
        return previewY * 4 + previewX;
      })
    );
  }
</script>

<div
  class="piece-preview"
  class:compact
  class:muted
  aria-label={label || undefined}
  aria-hidden={label ? undefined : true}
  style={`--piece-color:${pieceColor}`}
>
  {#each Array.from({ length: 8 }, (_, index) => index) as index}
    <span class:occupied={occupied.has(index)}></span>
  {/each}
</div>

<style>
  .piece-preview {
    display: grid;
    gap: 2px;
    grid-template-columns: repeat(4, 14px);
    grid-template-rows: repeat(2, 14px);
    justify-content: center;
    min-height: 30px;
  }

  span {
    border-radius: 2px;
    display: block;
  }

  span.occupied {
    background: var(--piece-color);
    box-shadow:
      inset 2px 2px 0 rgba(255, 255, 255, .17),
      inset -2px -2px 0 rgba(20, 26, 24, .24);
  }

  .compact {
    gap: 1px;
    grid-template-columns: repeat(4, 10px);
    grid-template-rows: repeat(2, 10px);
    min-height: 21px;
  }

  .muted span.occupied {
    background: #88918d;
    box-shadow:
      inset 2px 2px 0 rgba(255, 255, 255, .13),
      inset -2px -2px 0 rgba(20, 26, 24, .2);
  }
</style>
