<script lang="ts">
  import { SearchX } from '@lucide/svelte';
  import { onDestroy } from 'svelte';

  import type {
    ClearraDiagnostic,
    ClearraForwardSearchOutcome,
    ClearraSearchProgressTelemetry,
    ClearraWasmSearchReport
  } from '../wasm/wasmCommandClient';
  import type { ForwardDamageAggregation } from './forwardSearchModel';
  import { replayForwardPlacementBoard } from './forwardPlacementBoard';
  import ResultWorkspaceFrame from './ResultWorkspaceFrame.svelte';
  import SolutionCopyButton from './SolutionCopyButton.svelte';
  import SolutionCopyFormatControl from './SolutionCopyFormatControl.svelte';
  import type {
    SolutionCopyFormat,
    SolutionExportPage
  } from './solutionExport';
  import {
    workspaceMessage,
    type WorkspaceLanguage
  } from './workspaceI18n';
  import type { WorkspaceRuntimeStatus } from './workspaceRuntime';

  export let report: ClearraWasmSearchReport | null;
  export let diagnostics: ClearraDiagnostic[] = [];
  export let status: WorkspaceRuntimeStatus;
  export let error = '';
  export let elapsedMs = 0;
  export let progressLabel = '';
  export let progressDone = 0;
  export let progressTotal = 0;
  export let progressTelemetry: ClearraSearchProgressTelemetry | null = null;
  export let forwardPatternDone = 0;
  export let forwardPatternTotal = 0;
  export let language: WorkspaceLanguage;
  export let height: number;
  export let initialBoardMask: bigint;
  export let tool: 'damage' | 'spin-finder';
  export let damageAggregation: ForwardDamageAggregation = 'maximum';
  export let minimumDamage = 0;

  const RESULT_BATCH_SIZE = 100;

  type PreparedForwardOutcome = {
    outcome: ClearraForwardSearchOutcome;
    board: ReturnType<typeof replayForwardPlacementBoard>;
  };

  let visibleCount = RESULT_BATCH_SIZE;
  let preparedResults: PreparedForwardOutcome[] = [];
  let preparedReport: ClearraWasmSearchReport | null | undefined;
  let preparedOutcomes: ClearraForwardSearchOutcome[] | undefined;
  let preparedTool: 'damage' | 'spin-finder' | undefined;
  let preparedHeight: number | undefined;
  let preparedInitialBoardMask: bigint | undefined;
  let preparationGeneration = 0;
  let preparationFrame: number | null = null;
  let preparationTimer: ReturnType<typeof setTimeout> | null = null;
  let copyFormat: SolutionCopyFormat = 'fumen';
  $: label = (key: Parameters<typeof workspaceMessage>[1], values: Record<string, string | number> = {}) => workspaceMessage(language, key, values);
  $: outcomes = report?.forward_search_kind === tool ? report.forward_outcomes : [];
  $: resultInitialBoardMask = parseInitialBoard(report?.forward_initial_board_mask, initialBoardMask);
  $: if (
    report !== preparedReport ||
    outcomes !== preparedOutcomes ||
    tool !== preparedTool ||
    height !== preparedHeight ||
    resultInitialBoardMask !== preparedInitialBoardMask
  ) {
    resetPreparedResults(outcomes, resultInitialBoardMask);
  }
  $: nextRevealCount = Math.min(
    RESULT_BATCH_SIZE,
    Math.max(0, outcomes.length - visibleCount)
  );
  $: nextBatchPrepared = preparedResults.length >= visibleCount + nextRevealCount;
  $: active = status === 'validating' || status === 'running' || status === 'cancelling';
  $: progressDetail = !active || progressTelemetry
    ? ''
    : (progressLabel === 'forward-search'
      ? label('forwardProgressStates', { count: progressDone.toLocaleString(language) })
      : label('forwardProgressStarting'));

  onDestroy(cancelScheduledPreparation);

  function resetPreparedResults(
    nextOutcomes: ClearraForwardSearchOutcome[],
    nextInitialBoardMask: bigint
  ) {
    cancelScheduledPreparation();
    preparationGeneration += 1;
    preparedReport = report;
    preparedOutcomes = nextOutcomes;
    preparedTool = tool;
    preparedHeight = height;
    preparedInitialBoardMask = nextInitialBoardMask;
    visibleCount = Math.min(RESULT_BATCH_SIZE, nextOutcomes.length);
    preparedResults = prepareOutcomeRange(
      nextOutcomes,
      0,
      Math.min(nextOutcomes.length, RESULT_BATCH_SIZE * 2),
      nextInitialBoardMask,
      height
    );
  }

  function prepareOutcomeRange(
    source: ClearraForwardSearchOutcome[],
    start: number,
    end: number,
    boardMask: bigint,
    boardHeight: number
  ): PreparedForwardOutcome[] {
    return source.slice(start, end).map((outcome) => ({
      outcome,
      board: replayForwardPlacementBoard(boardMask, boardHeight, outcome.path)
    }));
  }

  function showMore() {
    const nextVisibleCount = Math.min(
      outcomes.length,
      visibleCount + RESULT_BATCH_SIZE
    );
    if (preparedResults.length < nextVisibleCount) return;
    visibleCount = nextVisibleCount;
    scheduleNextPreparation();
  }

  function scheduleNextPreparation() {
    cancelScheduledPreparation();
    const targetCount = Math.min(
      outcomes.length,
      visibleCount + RESULT_BATCH_SIZE
    );
    if (preparedResults.length >= targetCount) return;

    const generation = preparationGeneration;
    const source = outcomes;
    const boardMask = resultInitialBoardMask;
    const boardHeight = height;
    const prepare = () => {
      preparationTimer = null;
      if (
        generation !== preparationGeneration ||
        source !== outcomes ||
        boardMask !== resultInitialBoardMask ||
        boardHeight !== height
      ) {
        return;
      }
      const start = preparedResults.length;
      if (start >= targetCount) return;
      preparedResults = [
        ...preparedResults,
        ...prepareOutcomeRange(source, start, targetCount, boardMask, boardHeight)
      ];
    };

    if (typeof requestAnimationFrame === 'function') {
      preparationFrame = requestAnimationFrame(() => {
        preparationFrame = null;
        preparationTimer = setTimeout(prepare, 0);
      });
    } else {
      preparationTimer = setTimeout(prepare, 0);
    }
  }

  function cancelScheduledPreparation() {
    if (preparationFrame !== null) {
      cancelAnimationFrame(preparationFrame);
      preparationFrame = null;
    }
    if (preparationTimer !== null) {
      clearTimeout(preparationTimer);
      preparationTimer = null;
    }
  }

  function groupLabel(outcome: ClearraForwardSearchOutcome): string {
    if (outcome.group === 't') return 'T';
    if (outcome.group === 'other') return label('nonTPieces');
    if (outcome.group === 'integrated') return label('integratedSpins');
    return '';
  }

  function parseInitialBoard(value: string | null | undefined, fallback: bigint): bigint {
    try {
      return value ? BigInt(value) : fallback;
    } catch {
      return fallback;
    }
  }

  async function loadAllOutcomePages(
    signal?: AbortSignal
  ): Promise<SolutionExportPage[]> {
    const pages: SolutionExportPage[] = [];
    for (let offset = 0; offset < outcomes.length; offset += RESULT_BATCH_SIZE) {
      throwIfAborted(signal);
      const end = Math.min(outcomes.length, offset + RESULT_BATCH_SIZE);
      for (let index = offset; index < end; index += 1) {
        const board = replayForwardPlacementBoard(
          resultInitialBoardMask,
          height,
          outcomes[index].path
        );
        if (board) pages.push(board.page);
      }
      if (end < outcomes.length) await nextPaint();
    }
    throwIfAborted(signal);
    return pages;
  }

  function nextPaint(): Promise<void> {
    return new Promise((resolve) => requestAnimationFrame(() => resolve()));
  }

  function throwIfAborted(signal: AbortSignal | undefined) {
    if (!signal?.aborted) return;
    if (signal.reason instanceof Error) throw signal.reason;
    const error = new Error('Solution copy was aborted.');
    error.name = 'AbortError';
    throw error;
  }
</script>

<ResultWorkspaceFrame
  ariaLabel={label('results')}
  {status}
  statusLabel={label(status)}
  elapsedLabel={label('elapsed')}
  elapsedText={`${(elapsedMs / 1000).toFixed(1)}s`}
  progressProfile={tool === 'damage' ? 'damage' : 'spin'}
  {language}
  {progressLabel}
  {progressDetail}
  {progressDone}
  {progressTotal}
  {progressTelemetry}
  {forwardPatternDone}
  {forwardPatternTotal}
  failureDiagnostics={diagnostics}
  failureMessage={error}
>
  {#if !active && status !== 'failed' && status !== 'terminated'}
    {#if !report || report.forward_search_kind !== tool}
      <div class="empty"><SearchX size={20} strokeWidth={1.7} /><p>{label('noForwardResult')}</p></div>
    {:else}
      <header class="result-header">
      <h2>{label(tool === 'damage' ? 'maximumDamage' : 'spinFinder')}</h2>
      <dl>
        {#if tool === 'damage'}
          <div><dt>{label('maximumDamage')}</dt><dd>{report.maximum_damage ?? '-'}</dd></div>
          {#if damageAggregation === 'at-least'}<div><dt>{label('minimumDamage')}</dt><dd>{minimumDamage}</dd></div>{/if}
        {/if}
        <div><dt>{label(tool === 'damage' ? (damageAggregation === 'maximum' ? 'bestRoutes' : 'matchingDamageRoutes') : 'spinResults')}</dt><dd>{outcomes.length.toLocaleString(language)}</dd></div>
      </dl>
      </header>
      <div class="copy-format-row">
        <SolutionCopyFormatControl
          bind:value={copyFormat}
          {language}
          loadPages={outcomes.length ? loadAllOutcomePages : null}
        />
      </div>

      {#if outcomes.length === 0}
        <div class="empty"><SearchX size={20} strokeWidth={1.7} /><p>{label('noForwardSolutions')}</p></div>
      {:else}
        <div class="outcome-grid">
          {#each preparedResults as result, index}
            <article hidden={index >= visibleCount}>
              <div class="card-heading">
                <strong>{tool === 'damage' ? label('damageRoute', { number: index + 1 }) : label('spinResult', { number: index + 1 })}</strong>
                <div class="card-actions">
                  {#if tool === 'damage'}<b>{result.outcome.total_damage} {label('damage')}</b>{:else}<b>{groupLabel(result.outcome)} · {result.outcome.spin_lines}L{result.outcome.spin_mini ? ` · ${label('mini')}` : ''}</b>{/if}
                  <SolutionCopyButton
                    page={result.board?.page ?? null}
                    format={copyFormat}
                    {language}
                  />
                </div>
              </div>
              {#if tool === 'spin-finder'}<p class="source-queue">{label('sourceQueue')}: <b>{result.outcome.source_queue}</b></p>{/if}
              {#if result.board}
                <div class="board" style={`--rows:${result.board.height};aspect-ratio:${10 / result.board.height}`} aria-label={label('minoPlacement')}>
                  {#each result.board.cells as cell}
                    <i class:empty={cell === null} class:existing={cell === 'G'} class={`piece-${cell ?? 'empty'}`}></i>
                  {/each}
                </div>
              {/if}
              <ol>
                {#each result.outcome.path as step}
                  <li><span>{step.piece} · R{step.rotation} · ({step.x}, {step.y})</span><em>{step.cleared_lines}L{step.spin_piece ? ` · ${step.spin_piece}${step.spin_mini ? ' mini' : ''}` : ''}{tool === 'damage' ? ` · +${step.damage}` : ''}</em></li>
                {/each}
              </ol>
            </article>
          {/each}
        </div>
        {#if visibleCount < outcomes.length}
          <button
            class="more"
            type="button"
            disabled={!nextBatchPrepared}
            aria-busy={!nextBatchPrepared}
            on:click={showMore}
          >{label('showMore', { count: nextRevealCount })}</button>
        {/if}
      {/if}
    {/if}
  {/if}
</ResultWorkspaceFrame>

<style>
  .result-header { align-items: end; border-bottom: 1px solid #dce2de; display: flex; gap: 24px; justify-content: space-between; padding-bottom: 18px; }
  .result-header h2 { color: #17211e; font-size: 15px; margin: 0; }
  dt { color: #697570; font-size: 10px; }
  .copy-format-row { margin-top: 16px; }
  dl { display: flex; gap: 30px; margin: 0; }
  dl div { display: grid; gap: 3px; text-align: right; }
  dd { color: #123f3a; font-size: 17px; font-weight: 800; margin: 0; }
  .outcome-grid { display: grid; gap: 14px; grid-template-columns: repeat(auto-fill, minmax(270px, 1fr)); margin-top: 20px; }
  article { border: 1px solid #d3dbd6; border-radius: 6px; min-width: 0; padding: 14px; }
  article[hidden] { display: none; }
  .card-heading { align-items: center; display: flex; font-size: 11px; gap: 12px; justify-content: space-between; margin-bottom: 10px; }
  .card-heading b { color: #086c64; }
  .card-actions { align-items: center; display: flex; gap: 6px; }
  .source-queue { color: #697570; font: 10px ui-monospace, SFMono-Regular, Consolas, monospace; margin: -4px 0 9px; }
  .source-queue b { color: #23433e; }
  .board { background: #111918; display: grid; gap: 0; grid-template-columns: repeat(10, minmax(0, 1fr)); grid-template-rows: repeat(var(--rows), minmax(0, 1fr)); margin: 0 auto; overflow: hidden; width: min(100%, 230px); }
  .board i { background: var(--cell-color); box-shadow: 0 0 0 .5px var(--cell-color); min-height: 0; min-width: 0; }
  .board i.empty { --cell-color: #1d2927; box-shadow: inset 0 0 0 1px rgba(216,226,222,.16); }
  .board i.existing { --cell-color: #78817e; }
  .piece-I { --cell-color: #60d6db; }
  .piece-O { --cell-color: #f2cb52; }
  .piece-T { --cell-color: #c47bdc; }
  .piece-S { --cell-color: #70c982; }
  .piece-Z { --cell-color: #ec7771; }
  .piece-J { --cell-color: #6d91e5; }
  .piece-L { --cell-color: #eaa05d; }
  ol { display: grid; gap: 4px; list-style: none; margin: 12px 0 0; max-height: 122px; overflow: auto; padding: 0; }
  li { align-items: center; background: #f1f4f2; color: #34423e; display: flex; font-size: 10px; gap: 8px; justify-content: space-between; padding: 6px 7px; }
  li em { color: #60706a; font-style: normal; text-align: right; }
  .empty { align-items: center; color: #77827d; display: flex; gap: 9px; justify-content: center; min-height: 130px; }
  .empty p { margin: 0; }
  .more { background: #fff; border: 1px solid #aebbb5; border-radius: 5px; color: #23433e; cursor: pointer; display: block; font-size: 11px; font-weight: 750; margin: 18px auto 0; min-height: 34px; padding: 0 14px; }
  .more:disabled { cursor: wait; opacity: .6; }
  @media (max-width: 720px) { .result-header { align-items: stretch; flex-direction: column; } dl { justify-content: space-between; } }
</style>
