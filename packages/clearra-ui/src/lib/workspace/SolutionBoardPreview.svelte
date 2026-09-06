<script lang="ts">
  import { AlertTriangle } from '@lucide/svelte';

  import type { SolutionExportBoard } from './solutionExport';

  export let board: SolutionExportBoard | null = null;
  export let ariaLabel: string;
  export let invalidLabel: string;
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

</style>
