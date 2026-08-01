<script lang="ts">
  import { AlertTriangle } from '@lucide/svelte';
  import { tick } from 'svelte';

  import SolutionCopyButton from './SolutionCopyButton.svelte';
  import {
    parseSolutionKey,
    renderSolutionBoard,
    type SolutionCopyFormat,
    type SolutionExportBoard,
    type SolutionExportPage
  } from './solutionExport';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';

  export let solutionKeys: string[] = [];
  export let solutionCount = solutionKeys.length;
  export let loadSolutionPage:
    | ((offset: number, limit: number) => Promise<{ keys: string[]; total: number }>)
    | null = null;
  export let solutionProbabilities: Record<
    string,
    {
      probability: string;
      probability_complete: boolean;
    }
  > = {};
  export let solutionAverageScores: Record<
    string,
    {
      average_score: string;
      score_complete: boolean;
    }
  > = {};
  export let solutionComments: Record<string, string> = {};
  export let solutionSetHash = '';
  export let targetLines = 4;
  export let language: WorkspaceLanguage;
  export let copyFormat: SolutionCopyFormat = 'ctk';

  const PAGE_SIZE = 100;

  type SolutionView = {
    board: SolutionExportBoard | null;
    page: SolutionExportPage | null;
  };

  type PreparedSolution = SolutionView & {
    index: number;
    key: string;
  };

  let visibleCount = PAGE_SIZE;
  let preparedCount = PAGE_SIZE * 2;
  let preparedSolutions: PreparedSolution[] = [];
  let lastSetIdentity = '';
  let solutionViewCache = new Map<string, SolutionView>();
  let loadedSolutionKeys: string[] = [];
  let loadingMore = false;
  let pageTotal = solutionCount;
  let pageLoadFailed = false;

  $: label = (
    key: Parameters<typeof workspaceMessage>[1],
    values: Record<string, string | number> = {}
  ) => workspaceMessage(language, key, values);
  $: setIdentity = `${solutionSetHash || 'unhashed'}:${fallbackSetIdentity(solutionKeys)}:${solutionCount}:${targetLines}`;
  $: if (setIdentity !== lastSetIdentity) {
    lastSetIdentity = setIdentity;
    visibleCount = PAGE_SIZE;
    loadedSolutionKeys = solutionKeys.slice();
    pageTotal = solutionCount;
    pageLoadFailed = false;
    preparedCount = Math.min(loadedSolutionKeys.length, PAGE_SIZE * 2);
    solutionViewCache = new Map<string, SolutionView>();
  }
  $: totalSolutionCount = Math.max(solutionCount, pageTotal, loadedSolutionKeys.length);
  $: minimumPreparedCount = Math.min(
    loadedSolutionKeys.length,
    Math.max(PAGE_SIZE * 2, visibleCount)
  );
  $: if (preparedCount < minimumPreparedCount) preparedCount = minimumPreparedCount;
  $: if (preparedCount > loadedSolutionKeys.length) preparedCount = loadedSolutionKeys.length;
  $: preparedSolutions = loadedSolutionKeys.slice(0, preparedCount).map((key, index) => ({
    ...solutionView(key, targetLines, solutionViewCache),
    index,
    key
  }));
  $: visibleSolutions = preparedSolutions.slice(0, visibleCount).map((solution) => ({
    ...solution,
    probability: solutionProbabilities[solution.key],
    averageScore: solutionAverageScores[solution.key],
    comment: solutionComments[solution.key]
  }));
  $: remainingCount = Math.max(0, totalSolutionCount - visibleSolutions.length);
  $: nextPageCount = Math.min(PAGE_SIZE, remainingCount);

  function fallbackSetIdentity(keys: string[]): string {
    return `${keys.length}:${keys[0] ?? ''}:${keys[keys.length - 1] ?? ''}`;
  }

  async function showMore() {
    if (loadingMore) return;
    const identity = setIdentity;
    const nextVisibleCount = Math.min(totalSolutionCount, visibleCount + PAGE_SIZE);
    loadingMore = true;
    try {
      await ensureLoaded(Math.min(totalSolutionCount, nextVisibleCount + PAGE_SIZE));
      pageLoadFailed = false;
    } catch {
      pageLoadFailed = true;
      return;
    } finally {
      loadingMore = false;
    }
    if (identity !== setIdentity) return;
    visibleCount = Math.min(loadedSolutionKeys.length, nextVisibleCount);
    await tick();
    if (identity !== setIdentity) return;
    preparedCount = Math.min(loadedSolutionKeys.length, visibleCount + PAGE_SIZE);
  }

  async function ensureLoaded(target: number) {
    if (!loadSolutionPage) return;
    while (loadedSolutionKeys.length < target) {
      const offset = loadedSolutionKeys.length;
      const response = await loadSolutionPage(offset, Math.min(PAGE_SIZE, target - offset));
      if (!response.keys.length) break;
      loadedSolutionKeys = [...loadedSolutionKeys, ...response.keys];
      pageTotal = Math.max(pageTotal, response.total);
    }
  }

  function probabilityLabel(value: string): string {
    const probability = Number(value);
    if (!Number.isFinite(probability)) return value;
    return new Intl.NumberFormat(language, {
      style: 'percent',
      maximumFractionDigits: 4
    }).format(probability);
  }

  function scoreLabel(value: string): string {
    const score = Number(value);
    if (!Number.isFinite(score)) return value;
    return new Intl.NumberFormat(language, {
      maximumFractionDigits: 4
    }).format(score);
  }

  function pageWithComment(
    page: SolutionExportPage | null,
    comment: string | undefined
  ): SolutionExportPage | null {
    return page && comment ? { ...page, comment } : page;
  }

  function solutionView(
    key: string,
    lines: number,
    cache: Map<string, SolutionView>
  ): SolutionView {
    const cached = cache.get(key);
    if (cached) return cached;
    const page = parseSolutionKey(key);
    const view = {
      board: page ? renderSolutionBoard(page, lines) : null,
      page
    };
    cache.set(key, view);
    return view;
  }
</script>

{#if visibleSolutions.length}
  {#if totalSolutionCount > PAGE_SIZE}
    <p class="gallery-status">
      {label('resultLimited', { count: visibleSolutions.length, total: totalSolutionCount })}
    </p>
  {/if}

  <ol class="solution-gallery">
    {#each visibleSolutions as solution (`${solution.index}:${solution.key}`)}
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
            {#if solution.averageScore}
              <span class="solution-probability">
                {label('solutionAverageScore')}: {scoreLabel(solution.averageScore.average_score)}
                {#if !solution.averageScore.score_complete} ({label('incomplete')}){/if}
              </span>
            {/if}
          </div>
          <SolutionCopyButton
            page={pageWithComment(solution.page, solution.comment)}
            format={copyFormat}
            {language}
          />
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
      <button type="button" on:click={showMore} disabled={loadingMore}>
        {label('showMore', { count: nextPageCount })}
      </button>
    </div>
    {#if pageLoadFailed}
      <div class="invalid-solution" role="alert">
        <AlertTriangle size={18} />
        <span>{label('solutionPageLoadFailed')}</span>
      </div>
    {/if}
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

  @media (max-width: 390px) {
    .solution-gallery {
      grid-template-columns: 1fr;
    }
  }
</style>
