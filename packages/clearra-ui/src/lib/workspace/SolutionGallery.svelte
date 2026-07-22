<script lang="ts">
  import { AlertTriangle, Check, Copy } from '@lucide/svelte';

  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';

  export let solutionKeys: string[] = [];
  export let solutionProbabilities: Record<
    string,
    {
      probability: string;
      probability_complete: boolean;
    }
  > = {};
  export let solutionSetHash = '';
  export let targetLines = 4;
  export let language: WorkspaceLanguage;

  type Piece = 'I' | 'O' | 'T' | 'S' | 'Z' | 'J' | 'L';
  type BoardCell = Piece | 'G' | null;
  type SolutionBoard = {
    cells: BoardCell[];
    height: number;
  };
  type ParsedSolutionKey = {
    initialMask: bigint;
    placements: Array<{ mask: bigint; piece: Piece }>;
    minimumHeight: number;
    bitLimit: number;
  };

  const PAGE_SIZE = 100;
  const BOARD_WIDTH = 10;
  const COMPACT_KEY_PATTERN = /^ctk1\|initial=([0-9a-f]{16})\|placements=(.*)$/;
  const EXTENDED_KEY_PATTERN = /^ctk2\|height=([0-9]{1,2})\|initial=([0-9a-f]{64})\|placements=(.*)$/;
  const COMPACT_PLACEMENT_PATTERN = /^([IOTSZJL]):([0-9a-f]{16})$/;
  const EXTENDED_PLACEMENT_PATTERN = /^([IOTSZJL]):([0-9a-f]{64})$/;

  let visibleCount = PAGE_SIZE;
  let copiedIndex = -1;
  let lastSetIdentity = '';

  $: label = (
    key: Parameters<typeof workspaceMessage>[1],
    values: Record<string, string | number> = {}
  ) => workspaceMessage(language, key, values);
  $: setIdentity = solutionSetHash || fallbackSetIdentity(solutionKeys);
  $: if (setIdentity !== lastSetIdentity) {
    lastSetIdentity = setIdentity;
    visibleCount = PAGE_SIZE;
  }
  $: visibleSolutions = solutionKeys.slice(0, visibleCount).map((key, index) => ({
    board: parseSolutionBoard(key, targetLines),
    index,
    key,
    probability: solutionProbabilities[key]
  }));
  $: remainingCount = Math.max(0, solutionKeys.length - visibleSolutions.length);
  $: nextPageCount = Math.min(PAGE_SIZE, remainingCount);

  function fallbackSetIdentity(keys: string[]): string {
    return `${keys.length}:${keys[0] ?? ''}:${keys[keys.length - 1] ?? ''}`;
  }

  function showMore() {
    visibleCount = Math.min(solutionKeys.length, visibleCount + PAGE_SIZE);
  }

  function probabilityLabel(value: string): string {
    const probability = Number(value);
    if (!Number.isFinite(probability)) return value;
    return new Intl.NumberFormat(language, {
      style: 'percent',
      maximumFractionDigits: 4
    }).format(probability);
  }

  async function copySolutionKey(key: string, index: number) {
    await navigator.clipboard.writeText(key);
    copiedIndex = index;
    window.setTimeout(() => {
      if (copiedIndex === index) copiedIndex = -1;
    }, 1400);
  }

  function parseSolutionBoard(key: string, minimumHeight: number): SolutionBoard | null {
    const parsed = parseSolutionKey(key);
    if (!parsed) return null;
    let occupied = parsed.initialMask;
    for (const placement of parsed.placements) occupied |= placement.mask;
    const requestedHeight = Math.max(1, Math.min(24, Math.trunc(minimumHeight || 1)));
    const height = Math.max(
      requestedHeight,
      parsed.minimumHeight,
      highestOccupiedRow(occupied, parsed.bitLimit) + 1
    );
    const cells = Array<BoardCell>(height * BOARD_WIDTH).fill(null);
    writeMask(cells, height, parsed.initialMask, 'G');
    for (const placement of parsed.placements) {
      writeMask(cells, height, placement.mask, placement.piece);
    }
    return { cells, height };
  }

  function parseSolutionKey(key: string): ParsedSolutionKey | null {
    const compact = COMPACT_KEY_PATTERN.exec(key);
    const extended = compact ? null : EXTENDED_KEY_PATTERN.exec(key);
    if (!compact && !extended) return null;

    const minimumHeight = extended ? Number(extended[1]) : 1;
    if (!Number.isInteger(minimumHeight) || minimumHeight < 1 || minimumHeight > 24) return null;
    const initialHex = compact ? compact[1] : extended![2];
    const encoded = compact ? compact[2] : extended![3];
    const bitLimit = compact ? 64 : minimumHeight * BOARD_WIDTH;
    const placementLimit = compact ? 16 : 60;
    const placementPattern = compact ? COMPACT_PLACEMENT_PATTERN : EXTENDED_PLACEMENT_PATTERN;
    const initialMask = BigInt(`0x${initialHex}`);
    if (initialMask >> BigInt(bitLimit)) return null;
    const encodedPlacements = encoded ? encoded.split(',') : [];
    if (encodedPlacements.length > placementLimit) return null;

    const placements: Array<{ mask: bigint; piece: Piece }> = [];
    let occupied = initialMask;
    for (const value of encodedPlacements) {
      const placement = placementPattern.exec(value);
      if (!placement) return null;
      const mask = BigInt(`0x${placement[2]}`);
      if (
        mask === 0n ||
        mask >> BigInt(bitLimit) ||
        popcount(mask) !== 4 ||
        (occupied & mask) !== 0n
      ) return null;
      occupied |= mask;
      placements.push({ mask, piece: placement[1] as Piece });
    }
    return { initialMask, placements, minimumHeight, bitLimit };
  }

  function popcount(value: bigint): number {
    let count = 0;
    while (value !== 0n) {
      value &= value - 1n;
      count += 1;
    }
    return count;
  }

  function highestOccupiedRow(mask: bigint, bitLimit: number): number {
    for (let bit = bitLimit - 1; bit >= 0; bit -= 1) {
      if ((mask & (1n << BigInt(bit))) !== 0n) return Math.floor(bit / BOARD_WIDTH);
    }
    return 0;
  }

  function writeMask(cells: BoardCell[], height: number, mask: bigint, value: Exclude<BoardCell, null>) {
    for (let bit = 0; bit < height * BOARD_WIDTH; bit += 1) {
      if ((mask & (1n << BigInt(bit))) === 0n) continue;
      const x = bit % BOARD_WIDTH;
      const y = Math.floor(bit / BOARD_WIDTH);
      const displayIndex = (height - y - 1) * BOARD_WIDTH + x;
      cells[displayIndex] = value;
    }
  }
</script>

{#if visibleSolutions.length}
  {#if solutionKeys.length > PAGE_SIZE}
    <p class="gallery-status">
      {label('resultLimited', { count: visibleSolutions.length, total: solutionKeys.length })}
    </p>
  {/if}

  <ol class="solution-gallery">
    {#each visibleSolutions as solution}
      <li>
        <div class="solution-heading">
          <div>
            <strong>{label('solutionNumber', { number: solution.index + 1 })}</strong>
            {#if solution.probability}
              <span class="solution-probability">
                {label('solutionProbability')}: {probabilityLabel(solution.probability.probability)}
                {#if !solution.probability.probability_complete} ({label('incomplete')}){/if}
              </span>
            {/if}
          </div>
          <button
            type="button"
            title={label('copySolutionKey')}
            aria-label={label('copySolutionKey')}
            on:click={() => copySolutionKey(solution.key, solution.index)}
          >
            {#if copiedIndex === solution.index}<Check size={14} />{:else}<Copy size={14} />{/if}
          </button>
        </div>

        {#if solution.board}
          <div
            class="solution-board"
            role="img"
            aria-label={label('solutionBoard', { number: solution.index + 1 })}
            style={`--solution-rows:${solution.board.height}`}
          >
            {#each solution.board.cells as cell}
              <span class:empty={cell === null} class:garbage={cell === 'G'} class={`cell piece-${cell ?? 'empty'}`} aria-hidden="true"></span>
            {/each}
          </div>
        {:else}
          <div class="invalid-solution" role="status">
            <AlertTriangle size={18} />
            <span>{label('invalidSolutionKey')}</span>
          </div>
        {/if}
      </li>
    {/each}
  </ol>

  {#if remainingCount > 0}
    <div class="load-more-row">
      <button type="button" on:click={showMore}>
        {label('showMore', { count: nextPageCount })}
      </button>
    </div>
  {/if}
{/if}

<style>
  .gallery-status {
    color: #68736f;
    font-size: 12px;
    margin: 0 0 12px;
  }

  .solution-gallery {
    display: grid;
    gap: 12px;
    grid-template-columns: repeat(auto-fill, minmax(154px, 1fr));
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .solution-gallery li {
    background: #f3f5f4;
    border: 1px solid #d7ded9;
    border-radius: 6px;
    min-width: 0;
    padding: 10px;
  }

  .solution-heading {
    align-items: center;
    display: flex;
    justify-content: space-between;
    margin-bottom: 8px;
  }

  .solution-heading strong {
    color: #4d5955;
    font-size: 11px;
  }

  .solution-probability {
    color: #075f58;
    display: block;
    font-size: 10px;
    font-weight: 700;
    margin-top: 2px;
  }

  .solution-heading button {
    align-items: center;
    background: transparent;
    border: 0;
    color: #50605a;
    cursor: pointer;
    display: inline-flex;
    height: 28px;
    justify-content: center;
    padding: 0;
    width: 28px;
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

  .cell.empty {
    --cell-color: #edf1ef;
  }

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
    color: #9a4e43;
    display: flex;
    font-size: 11px;
    gap: 8px;
    min-height: 72px;
    padding: 10px;
  }

  .load-more-row {
    display: flex;
    justify-content: center;
    padding-top: 18px;
  }

  .load-more-row button {
    background: #ffffff;
    border: 1px solid #aebbb6;
    border-radius: 5px;
    color: #174a45;
    cursor: pointer;
    font: inherit;
    font-size: 12px;
    font-weight: 750;
    min-height: 36px;
    padding: 0 16px;
  }

  @media (max-width: 520px) {
    .solution-gallery {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
