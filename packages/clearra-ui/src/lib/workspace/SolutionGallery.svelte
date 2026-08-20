<script lang="ts">
  import { AlertTriangle } from '@lucide/svelte';
  import { onDestroy, tick } from 'svelte';

  import SolutionCopyButton from './SolutionCopyButton.svelte';
  import {
    formatFinesseInputCount,
    representativeWitnessExportForSolution,
    type BuildProbabilitySolutionFinesse
  } from './buildProbabilityFinesse';
  import {
    parseSolutionKey,
    renderSolutionBoard,
    type SolutionCopyFormat,
    type SolutionExportBoard,
    type SolutionExportPage
  } from './solutionExport';
  import type { ClearraFinesseRepresentativeWitness } from '../wasm/wasmCommandClient';
  import {
    solutionPageResultIdentity,
    type SolutionPageLoader
  } from './solutionPageSource';
  import { workspaceMessage, type WorkspaceLanguage } from './workspaceI18n';

  export let solutionKeys: string[] = [];
  export let solutionCount = solutionKeys.length;
  export let loadSolutionPage: SolutionPageLoader | null = null;
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
  export let solutionFinesse: Record<string, BuildProbabilitySolutionFinesse[]> = {};
  export let representativeWitness: ClearraFinesseRepresentativeWitness | null = null;
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
  let pageLoadController: AbortController | null = null;

  onDestroy(() => abortPageLoads('Solution gallery was disposed.'));

  $: label = (
    key: Parameters<typeof workspaceMessage>[1],
    values: Record<string, string | number> = {}
  ) => workspaceMessage(language, key, values);
  $: setIdentity = `${solutionPageResultIdentity(
    solutionSetHash,
    solutionCount,
    solutionKeys
  )}:lines:${targetLines}`;
  $: if (setIdentity !== lastSetIdentity) {
    abortPageLoads('Solution gallery result was replaced.');
    pageLoadController = new AbortController();
    lastSetIdentity = setIdentity;
    visibleCount = PAGE_SIZE;
    loadedSolutionKeys = solutionKeys.slice(0, solutionCount);
    pageTotal = solutionCount;
    pageLoadFailed = false;
    loadingMore = false;
    preparedCount = Math.min(loadedSolutionKeys.length, PAGE_SIZE * 2);
    solutionViewCache = new Map<string, SolutionView>();
    if (loadedSolutionKeys.length === 0 && solutionCount > 0 && loadSolutionPage) {
      void primeInitialPage(setIdentity, pageLoadController);
    }
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
    finesse: solutionFinesse[solution.key] ?? [],
    comment: solutionComments[solution.key]
  }));
  $: remainingCount = Math.max(0, totalSolutionCount - visibleSolutions.length);
  $: nextPageCount = Math.min(PAGE_SIZE, remainingCount);

  async function showMore() {
    if (loadingMore) return;
    const identity = setIdentity;
    const controller = pageLoadController ?? new AbortController();
    pageLoadController = controller;
    const nextVisibleCount = Math.min(totalSolutionCount, visibleCount + PAGE_SIZE);
    loadingMore = true;
    try {
      await ensureLoaded(
        Math.min(totalSolutionCount, nextVisibleCount + PAGE_SIZE),
        identity,
        controller.signal
      );
      if (identity === setIdentity) pageLoadFailed = false;
    } catch (error) {
      if (isAbortError(error)) return;
      if (identity === setIdentity) pageLoadFailed = true;
      return;
    } finally {
      if (identity === setIdentity) loadingMore = false;
    }
    if (identity !== setIdentity) return;
    visibleCount = Math.min(loadedSolutionKeys.length, nextVisibleCount);
    await tick();
    if (identity !== setIdentity) return;
    preparedCount = Math.min(loadedSolutionKeys.length, visibleCount + PAGE_SIZE);
  }

  async function primeInitialPage(identity: string, controller: AbortController) {
    if (loadingMore) return;
    loadingMore = true;
    try {
      await ensureLoaded(
        Math.min(solutionCount, PAGE_SIZE * 2),
        identity,
        controller.signal
      );
      if (identity !== setIdentity) return;
      visibleCount = Math.min(PAGE_SIZE, loadedSolutionKeys.length);
      pageLoadFailed = false;
    } catch (error) {
      if (identity === setIdentity && !isAbortError(error)) pageLoadFailed = true;
    } finally {
      if (identity === setIdentity) loadingMore = false;
    }
  }

  async function ensureLoaded(target: number, identity: string, signal: AbortSignal) {
    if (!loadSolutionPage) return;
    while (loadedSolutionKeys.length < target) {
      throwIfAborted(signal);
      if (identity !== setIdentity) throw stalePageError();
      const offset = loadedSolutionKeys.length;
      const limit = Math.min(PAGE_SIZE, target - offset);
      const response = await loadSolutionPage(offset, limit, signal);
      throwIfAborted(signal);
      if (identity !== setIdentity) throw stalePageError();
      if (
        !Number.isSafeInteger(response.total) ||
        response.total !== solutionCount ||
        !Array.isArray(response.keys) ||
        response.keys.length === 0 ||
        response.keys.length > limit ||
        response.keys.some((key) => typeof key !== 'string')
      ) {
        throw new Error('Solution gallery page does not match the completed result.');
      }
      loadedSolutionKeys = [...loadedSolutionKeys, ...response.keys];
      pageTotal = response.total;
    }
  }

  function abortPageLoads(message: string) {
    if (!pageLoadController || pageLoadController.signal.aborted) return;
    const error = new Error(message);
    error.name = 'AbortError';
    pageLoadController.abort(error);
  }

  function throwIfAborted(signal: AbortSignal) {
    if (!signal.aborted) return;
    if (signal.reason instanceof Error) throw signal.reason;
    const error = new Error('Solution gallery page load was aborted.');
    error.name = 'AbortError';
    throw error;
  }

  function stalePageError() {
    const error = new Error('Solution gallery result was replaced.');
    error.name = 'AbortError';
    return error;
  }

  function isAbortError(error: unknown): boolean {
    return error instanceof Error && error.name === 'AbortError';
  }

  function probabilityLabel(value: string): string {
    const probability = Number(value);
    if (!Number.isFinite(probability) || probability < 0 || probability > 1) return '—';
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
            <div class="solution-metrics">
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
              {#each solution.finesse as finesse}
                <span class="solution-probability">
                  {label('finesseSolutionAverageInputs')}
                  ({label(finesse.policy === 'oracle' ? 'finesseOraclePolicy' : 'finesseVisibleSevenPolicy')}):
                  {formatFinesseInputCount(finesse.average_inputs, language)}
                  {#if !finesse.complete} ({label('finesseMaterialized')}){/if}
                </span>
              {/each}
            </div>
          </div>
          <SolutionCopyButton
            page={pageWithComment(solution.page, solution.comment)}
            finesseWitness={representativeWitnessExportForSolution(
              representativeWitness,
              solution.key,
              solution.finesse
            )}
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
    align-items: flex-start;
    display: flex;
    gap: 8px;
    justify-content: space-between;
    margin-bottom: 8px;
  }

  .solution-heading > div { min-width: 0; }

  .solution-heading strong {
    color: #4d5955;
    font-size: 11px;
  }

  .solution-metrics {
    display: flex;
    flex-wrap: wrap;
    gap: 2px 8px;
    margin-top: 2px;
  }

  .solution-probability {
    color: #075f58;
    font-size: 10px;
    font-weight: 700;
    overflow-wrap: anywhere;
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
