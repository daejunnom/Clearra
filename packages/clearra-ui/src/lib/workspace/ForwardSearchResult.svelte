<script lang="ts">
  import { CheckCircle2, SearchX } from '@lucide/svelte';

  import type {
    ClearraDiagnostic,
    ClearraForwardSearchOutcome,
    ClearraSearchProgressTelemetry,
    ClearraWasmSearchReport
  } from '../wasm/wasmCommandClient';
  import type { ForwardDamageAggregation } from './forwardSearchModel';
  import { replayForwardPlacementBoard } from './forwardPlacementBoard';
  import {
    workspaceMessage,
    workspaceProgressDetail,
    workspaceProgressLabel,
    type WorkspaceLanguage
  } from './workspaceI18n';
  import type { WorkspaceRuntimeStatus } from './workspaceRuntime';

  export let report: ClearraWasmSearchReport | null;
  export let diagnostics: ClearraDiagnostic[] = [];
  export let status: WorkspaceRuntimeStatus;
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

  let visible = 24;
  $: label = (key: Parameters<typeof workspaceMessage>[1], values: Record<string, string | number> = {}) => workspaceMessage(language, key, values);
  $: outcomes = report?.forward_outcomes ?? [];
  $: shown = outcomes.slice(0, visible);
  $: rendered = shown.map((outcome) => ({
    outcome,
    board: replayForwardPlacementBoard(
      parseInitialBoard(report?.forward_initial_board_mask, initialBoardMask),
      height,
      outcome.path
    )
  }));
  $: if (report?.forward_search_kind !== tool) visible = 24;
  $: active = status === 'validating' || status === 'running' || status === 'cancelling';
  $: distributedProgress = progressTelemetry !== null;
  $: patternProgress = !distributedProgress && (forwardPatternTotal > 1 || (progressLabel === 'forward-search-patterns' && progressTotal > 1));
  $: visiblePatternDone = forwardPatternTotal > 1 ? forwardPatternDone : progressDone;
  $: visiblePatternTotal = forwardPatternTotal > 1 ? forwardPatternTotal : progressTotal;
  $: progressPercent = patternProgress ? Math.min(100, (visiblePatternDone / visiblePatternTotal) * 100) : 0;
  $: progressPhase = status === 'cancelling'
    ? label('cancelling')
    : workspaceProgressLabel(language, progressTelemetry)
      ?? (patternProgress
        ? label('forwardProgressPatterns')
        : progressLabel === 'forward-search'
          ? label('forwardProgressSearching')
          : label('forwardProgressPreparing'));
  $: currentStateDetail = distributedProgress
    ? workspaceProgressDetail(language, progressTelemetry)
    : progressLabel === 'forward-search'
      ? label('forwardProgressStates', { count: progressDone.toLocaleString(language) })
      : '';
  $: patternDetail = patternProgress
    ? label('forwardProgressPatternCount', {
        done: visiblePatternDone.toLocaleString(language),
        total: visiblePatternTotal.toLocaleString(language)
      })
    : '';
  $: progressDetail = patternProgress
    ? [patternDetail, currentStateDetail].filter(Boolean).join(' · ')
    : currentStateDetail || label('forwardProgressStarting');

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
</script>

<section class="result-band" aria-label={label('results')}>
  {#if active}
    <div class="progress-panel" role="status" aria-live="polite">
      <div class="progress-heading">
        <span>{label('forwardProgress')}</span>
        <strong>{progressPhase}</strong>
      </div>
      <div
        class="progress-track"
        class:indeterminate={!patternProgress}
        role="progressbar"
        aria-label={label('forwardProgress')}
        aria-valuemin="0"
        aria-valuemax={patternProgress ? visiblePatternTotal : undefined}
        aria-valuenow={patternProgress ? visiblePatternDone : undefined}
        aria-valuetext={progressDetail}
      ><span style={patternProgress ? `width:${progressPercent}%` : ''}></span></div>
      <p>{progressDetail}</p>
    </div>
  {:else if !report || report.forward_search_kind !== tool}
    <div class="empty"><SearchX size={20} strokeWidth={1.7} /><p>{label('noForwardResult')}</p></div>
  {:else}
    <header class="result-header">
      <div><CheckCircle2 size={20} strokeWidth={1.8} /><span><small>{label('status')}</small><strong>{label('completed')}</strong></span></div>
      <dl>
        {#if tool === 'damage'}
          <div><dt>{label('maximumDamage')}</dt><dd>{report.maximum_damage ?? '-'}</dd></div>
          {#if damageAggregation === 'at-least'}<div><dt>{label('minimumDamage')}</dt><dd>{minimumDamage}</dd></div>{/if}
        {/if}
        <div><dt>{label(tool === 'damage' ? (damageAggregation === 'maximum' ? 'bestRoutes' : 'matchingDamageRoutes') : 'spinResults')}</dt><dd>{outcomes.length.toLocaleString(language)}</dd></div>
        <div><dt>{label('searchedNodes')}</dt><dd>{report.searched_nodes.toLocaleString(language)}</dd></div>
      </dl>
    </header>

    {#if outcomes.length === 0}
      <div class="empty"><SearchX size={20} strokeWidth={1.7} /><p>{label('noForwardSolutions')}</p></div>
    {:else}
      <div class="outcome-grid">
        {#each rendered as result, index}
          <article>
            <div class="card-heading">
              <strong>{tool === 'damage' ? label('damageRoute', { number: index + 1 }) : label('spinResult', { number: index + 1 })}</strong>
              {#if tool === 'damage'}<b>{result.outcome.total_damage} {label('damage')}</b>{:else}<b>{groupLabel(result.outcome)} · {result.outcome.spin_lines}L{result.outcome.spin_mini ? ` · ${label('mini')}` : ''}</b>{/if}
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
      {#if visible < outcomes.length}
        <button class="more" type="button" on:click={() => (visible += 24)}>{label('showMore', { count: Math.min(24, outcomes.length - visible) })}</button>
      {/if}
    {/if}
  {/if}

  {#if diagnostics.length}
    <div class="diagnostics"><strong>{label('diagnostics')}</strong>{#each diagnostics as diagnostic}<p><b>{diagnostic.code}</b> {diagnostic.message}</p>{/each}</div>
  {/if}
</section>

<style>
  .result-band { background: #fff; border-top: 1px solid #d5dcd7; padding: 24px max(24px, calc((100vw - 1460px) / 2)) 40px; }
  .progress-panel { border: 1px solid #d3dbd6; border-radius: 6px; padding: 16px; }
  .progress-heading { align-items: center; display: flex; gap: 16px; justify-content: space-between; }
  .progress-heading span { color: #697570; font-size: 10px; font-weight: 700; text-transform: uppercase; }
  .progress-heading strong { color: #174d47; font-size: 13px; }
  .progress-track { background: #e4e9e6; height: 5px; margin-top: 12px; overflow: hidden; position: relative; }
  .progress-track > span { background: #16877d; display: block; height: 100%; transition: width 120ms linear; }
  .progress-track.indeterminate > span { animation: forward-progress 1.1s ease-in-out infinite; left: 0; position: absolute; width: 34%; }
  .progress-panel p { color: #5f6c67; font-size: 11px; margin: 9px 0 0; }
  .result-header { align-items: end; border-bottom: 1px solid #dce2de; display: flex; gap: 24px; justify-content: space-between; padding-bottom: 18px; }
  .result-header > div { align-items: center; color: #08766d; display: flex; gap: 9px; }
  .result-header span { display: grid; gap: 2px; }
  .result-header small, dt { color: #697570; font-size: 10px; }
  .result-header strong { color: #17211e; font-size: 17px; }
  dl { display: flex; gap: 30px; margin: 0; }
  dl div { display: grid; gap: 3px; text-align: right; }
  dd { color: #123f3a; font-size: 17px; font-weight: 800; margin: 0; }
  .outcome-grid { display: grid; gap: 14px; grid-template-columns: repeat(auto-fill, minmax(270px, 1fr)); margin-top: 20px; }
  article { border: 1px solid #d3dbd6; border-radius: 6px; min-width: 0; padding: 14px; }
  .card-heading { align-items: center; display: flex; font-size: 11px; gap: 12px; justify-content: space-between; margin-bottom: 10px; }
  .card-heading b { color: #086c64; }
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
  .diagnostics { background: #fff4ee; color: #713d2b; font-size: 11px; margin-top: 18px; padding: 12px; }
  .diagnostics p { margin: 5px 0 0; }
  @keyframes forward-progress { 0% { transform: translateX(-110%); } 100% { transform: translateX(330%); } }
  @media (prefers-reduced-motion: reduce) { .progress-track.indeterminate > span { animation: none; left: 33%; } }
  @media (max-width: 720px) { .result-header { align-items: stretch; flex-direction: column; } dl { justify-content: space-between; } }
</style>
