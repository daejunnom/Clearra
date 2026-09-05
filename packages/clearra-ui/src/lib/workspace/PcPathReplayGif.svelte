<script lang="ts">
  import { AlertTriangle } from '@lucide/svelte';
  import { onDestroy, onMount } from 'svelte';

  import type { ClearraPcPathWitnessPayload } from '../wasm/wasmCommandClient';
  import { encodePcPathReplayGif } from './pcPathReplayGif';
  import { buildPcPathReplayFrames } from './pcPathReplayPresentation';

  export let witness: ClearraPcPathWitnessPayload;
  export let targetLines: number;
  export let expectedTerminalBoardMask: string | null = null;
  export let ariaLabel: string;
  export let invalidLabel: string;

  let mounted = false;
  let renderedIdentity = '';
  let gifUrl = '';
  let renderError = '';
  let frameCount = 0;

  $: replayIdentity = [
    witness.candidate_id,
    witness.pattern_id,
    witness.normalized_trace_key,
    targetLines,
    expectedTerminalBoardMask ?? 'pc-empty'
  ].join(':');
  $: if (mounted && replayIdentity !== renderedIdentity) renderReplay(replayIdentity);

  onMount(() => {
    mounted = true;
    renderReplay(replayIdentity);
  });
  onDestroy(() => {
    revokeGifUrl();
  });

  function renderReplay(identity: string) {
    renderedIdentity = identity;
    revokeGifUrl();
    renderError = '';
    frameCount = 0;
    try {
      const frames = expectedTerminalBoardMask === null
        ? buildPcPathReplayFrames(witness, targetLines)
        : buildPcPathReplayFrames(witness, targetLines, expectedTerminalBoardMask);
      const bytes = encodePcPathReplayGif(frames);
      gifUrl = URL.createObjectURL(new Blob([bytes], { type: 'image/gif' }));
      frameCount = frames.length;
    } catch (reason) {
      renderError = reason instanceof Error ? reason.message : String(reason);
    }
  }

  function revokeGifUrl() {
    if (gifUrl) URL.revokeObjectURL(gifUrl);
    gifUrl = '';
  }

</script>

<div class="pc-path-replay-gif">
  {#if gifUrl}
    <img src={gifUrl} alt={ariaLabel} width="200" height={Math.max(80, targetLines * 20)} />
    <span class="frame-count">{frameCount} frames · 500ms</span>
  {:else if renderError}
    <div class="invalid-replay" role="status">
      <AlertTriangle size={18} />
      <span>{invalidLabel}</span>
    </div>
  {/if}

</div>

<style>
  .pc-path-replay-gif { display: grid; gap: 7px; max-width: 240px; min-width: 0; width: 100%; }
  img { background: #1e2927; border: 1px solid #cbd3ce; border-radius: 4px; height: auto; image-rendering: pixelated; width: 100%; }
  .frame-count { color: #68736f; font-size: 10px; }
  .invalid-replay { align-items: center; background: #fff5e8; border: 1px solid #edcfaa; border-radius: 5px; color: #77501e; display: flex; font-size: 11px; gap: 7px; min-height: 54px; padding: 9px; }
</style>
