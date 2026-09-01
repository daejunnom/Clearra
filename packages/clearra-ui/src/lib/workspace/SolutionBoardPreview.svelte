<script lang="ts">
  import { AlertTriangle } from '@lucide/svelte';
  import { onDestroy } from 'svelte';

  import type { SolutionExportBoard } from './solutionExport';
  import { writeClipboardText } from './clipboardText';

  export let board: SolutionExportBoard | null = null;
  export let ariaLabel: string;
  export let invalidLabel: string;
  export let rawKey: string | null = null;
  export let rawKeyDetailsLabel = 'Show internal key';
  export let copyRawKeyLabel = 'Copy key';
  export let copiedRawKeyLabel = 'Copied';
  export let copyRawKeyFailedLabel = 'Copy failed';

  let copyState: 'idle' | 'copied' | 'failed' = 'idle';
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  onDestroy(clearCopyTimer);

  async function copyRawKey() {
    if (!rawKey) return;
    clearCopyTimer();
    try {
      await writeClipboardText(rawKey);
      copyState = 'copied';
    } catch {
      copyState = 'failed';
    }
    copyTimer = setTimeout(() => {
      copyState = 'idle';
      copyTimer = null;
    }, 1600);
  }

  function clearCopyTimer() {
    if (copyTimer !== null) clearTimeout(copyTimer);
    copyTimer = null;
  }
</script>

<div class="solution-board-preview">
  {#if board}
    <div
      class="solution-board"
      role="img"
      aria-label={ariaLabel}
      style={`--solution-rows:${board.height}`}
    >
      {#each board.cells as cell}
        <span
          class:empty={cell === null}
          class:garbage={cell === 'G'}
          class={`cell piece-${cell ?? 'empty'}`}
          aria-hidden="true"
        ></span>
      {/each}
    </div>
  {:else}
    <div class="invalid-solution" role="status">
      <AlertTriangle size={18} />
      <span>{invalidLabel}</span>
    </div>
  {/if}

  {#if rawKey}
    <details class="raw-key-details">
      <summary>{rawKeyDetailsLabel}</summary>
      <div>
        <code data-role="raw-solution-key">{rawKey}</code>
        <button type="button" on:click={copyRawKey}>
          {copyState === 'copied'
            ? copiedRawKeyLabel
            : copyState === 'failed'
              ? copyRawKeyFailedLabel
              : copyRawKeyLabel}
        </button>
      </div>
    </details>
  {/if}
</div>

<style>
  .solution-board-preview {
    display: grid;
    gap: 7px;
    min-width: 0;
    width: 100%;
  }

  .solution-board {
    aspect-ratio: calc(10 / var(--solution-rows));
    background: #cbd3ce;
    border: 1px solid #cbd3ce;
    border-radius: 4px;
    display: grid;
    gap: 0;
    grid-template-columns: repeat(10, minmax(0, 1fr));
    grid-template-rows: repeat(var(--solution-rows), minmax(0, 1fr));
    overflow: hidden;
    width: 100%;
  }

  .cell {
    background: var(--cell-color);
    box-shadow: 0 0 0 0.5px var(--cell-color);
    min-height: 0;
    min-width: 0;
  }

  .cell.empty { --cell-color: #edf1ef; }
  .cell.garbage { --cell-color: #78817e; }
  .piece-I { --cell-color: #60d6db; }
  .piece-O { --cell-color: #f2cb52; }
  .piece-T { --cell-color: #c47bdc; }
  .piece-S { --cell-color: #70c982; }
  .piece-Z { --cell-color: #ec7771; }
  .piece-J { --cell-color: #6d91e5; }
  .piece-L { --cell-color: #eaa05d; }

  .invalid-solution {
    align-items: center;
    background: #fff5e8;
    border: 1px solid #edcfaa;
    border-radius: 5px;
    color: #77501e;
    display: flex;
    font-size: 11px;
    gap: 7px;
    min-height: 54px;
    padding: 9px;
  }

  .raw-key-details {
    color: #68736f;
    font-size: 10px;
    min-width: 0;
  }

  .raw-key-details summary {
    cursor: pointer;
    font-weight: 700;
  }

  .raw-key-details > div {
    align-items: flex-start;
    display: flex;
    gap: 6px;
    margin-top: 5px;
    min-width: 0;
  }

  .raw-key-details code {
    background: #eef2f0;
    border-radius: 3px;
    flex: 1;
    overflow-wrap: anywhere;
    padding: 4px 5px;
    user-select: all;
  }

  .raw-key-details button {
    background: #fff;
    border: 1px solid #cbd3ce;
    border-radius: 4px;
    color: #35443f;
    cursor: pointer;
    flex: 0 0 auto;
    font-size: 10px;
    min-height: 26px;
    padding: 3px 7px;
  }
</style>
