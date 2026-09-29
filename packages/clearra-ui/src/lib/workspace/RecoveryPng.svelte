<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { renderSolutionBoard, type SolutionExportPage } from './solutionExport';
  import { rasterizePcPathReplayFrame } from './pcPathReplayGif';
  export let page: SolutionExportPage;
  export let ariaLabel: string;
  export let invalidLabel: string;
  let mounted = false;
  let url = '';
  let failed = false;
  let generation = 0;
  $: if (mounted) void render(page);
  onMount(() => { mounted = true; });
  onDestroy(() => { generation++; if (url) URL.revokeObjectURL(url); });
  async function render(value: SolutionExportPage) {
    const token = ++generation;
    if (url) URL.revokeObjectURL(url);
    url = ''; failed = false;
    try {
      const board = renderSolutionBoard(value, value.height);
      // The export board is top-down; the common replay raster is bottom-up.
      const cells = [];
      for (let y = board.height - 1; y >= 0; y--) cells.push(...board.cells.slice(y * 10, y * 10 + 10));
      const image = rasterizePcPathReplayFrame({ width: 10, height: board.height, cells, phase: 'lock', stepIndex: null });
      const canvas = document.createElement('canvas');
      canvas.width = image.width; canvas.height = image.height;
      const context = canvas.getContext('2d');
      if (!context) throw new Error('PNG canvas unavailable');
      context.putImageData(new ImageData(new Uint8ClampedArray(image.rgba), image.width, image.height), 0, 0);
      const blob = await new Promise<Blob>((resolve, reject) => canvas.toBlob(value => value ? resolve(value) : reject(new Error('PNG encoding failed')), 'image/png'));
      if (token === generation) url = URL.createObjectURL(blob);
    } catch { if (token === generation) failed = true; }
  }
</script>
{#if url}<img src={url} alt={ariaLabel} />{:else if failed}<p class="invalid-replay" role="status">{invalidLabel}</p>{/if}
<style>img { width: 100%; max-width: 240px; height: auto; image-rendering: pixelated; border: 1px solid #cbd3ce; border-radius: 4px; }</style>
