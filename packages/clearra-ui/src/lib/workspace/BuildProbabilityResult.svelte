<script lang="ts">
  import { ArrowDownToLine, CheckCircle2, Search } from '@lucide/svelte';
  import { createEventDispatcher } from 'svelte';

  import ResultWorkspaceFrame from './ResultWorkspaceFrame.svelte';
  import SolutionCopyFormatControl from './SolutionCopyFormatControl.svelte';
  import SolutionGallery from './SolutionGallery.svelte';
  import type { SolutionCopyFormat } from './solutionExport';
  import { boardCellOccupied } from './solverWorkspaceModel';
  import type { WorkspaceRuntimeView } from './workspaceRuntime';
  import {
    workspaceMessage,
    workspaceProbability,
    workspaceProgressDetail,
    workspaceProgressLabel,
    type WorkspaceLanguage
  } from './workspaceI18n';

  export let view: WorkspaceRuntimeView;
  export let language: WorkspaceLanguage;
  export let elapsedMs = 0;
  export let height = 8;
  export let existingMask = 0n;
  export let targetMask = 0n;
  export let aggregation: 'buildability' | 'spin' = 'buildability';

  const dispatch = createEventDispatcher<{ continue: { existingMask: bigint; height: number } }>();
  const columns = Array.from({ length: 10 }, (_, index) => index);
  let copyFormat: SolutionCopyFormat = 'fumen';

  $: rows = Array.from({ length: height }, (_, index) => height - index - 1);
  $: report = view.searchReport;
  $: summary = Object.fromEntries(report?.summary_fields ?? []);
  $: solutionKeys = report?.normalized_solution_keys ?? [];
  $: finalBoardMask = parseBoardMask(summary.build_final_board_mask);
  $: canContinue = view.status === 'completed' && report?.solution_found === true && finalBoardMask !== null;
  $: hasOutput = Boolean(view.response || report || view.diagnostics.length || view.error);
  $: label = (
    key: Parameters<typeof workspaceMessage>[1],
    values: Record<string, string | number> = {}
  ) => workspaceMessage(language, key, values);

  function number(value: number | undefined): string {
    return value === undefined ? '—' : new Intl.NumberFormat(language).format(value);
  }

  function bytes(value: number | undefined): string {
    if (value === undefined) return '—';
    return `${(value / 1_048_576).toFixed(value >= 104_857_600 ? 0 : 1)} MiB`;
  }

  function summaryNumber(value: string | undefined): number | undefined {
    if (value === undefined) return undefined;
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : undefined;
  }

  function parseBoardMask(value: string | undefined): bigint | null {
    if (!value) return null;
    try {
      return BigInt(value);
    } catch {
      return null;
    }
  }

  function continueFromResult() {
    if (finalBoardMask === null) return;
    dispatch('continue', { existingMask: finalBoardMask, height });
  }
</script>

<ResultWorkspaceFrame
  ariaLabel={label('buildProbabilityResults')}
  status={view.status}
  statusLabel={label(view.status)}
  elapsedLabel={label('elapsed')}
  elapsedText={`${(elapsedMs / 1000).toFixed(1)}s`}
  runtimeTitle={label('runtime')}
  runtimeLabel={label(view.kind === 'web' ? 'runtimeWeb' : 'runtimeDesktop')}
  progressAriaLabel={label('progress')}
  progressLabel={(workspaceProgressLabel(language, view.progressTelemetry) ?? view.progressLabel) || label('idle')}
  progressDetail={workspaceProgressDetail(language, view.progressTelemetry)}
  progressDone={view.progressDone}
  progressTotal={view.progressTotal}
  progressDoneText={number(view.progressDone)}
  progressTotalText={number(view.progressTotal)}
  overviewLabel={label('overview')}
  solutionsLabel={label('solutions')}
  solutionCountText={number(solutionKeys.length)}
  diagnosticsLabel={label('diagnostics')}
  diagnosticCountText={number(view.diagnostics.length)}
  let:activeTab
>
    {#if !hasOutput && view.status === 'idle'}
      <div class="empty-state"><Search size={28} strokeWidth={1.5} /><p>{label('noBuildProbabilityResult')}</p></div>
    {:else if activeTab === 'overview'}
      <div class="result-grid">
        <div class="preview-panel">
          <h3>{label(canContinue ? 'clearedBuildResult' : 'requestedBuild')}</h3>
          <div class="board-frame">
            <div class="board" style={`--board-rows:${height}`}>
              {#each rows as y}
                {#each columns as x}
                  {#if canContinue && finalBoardMask !== null}
                    <span class:existing={boardCellOccupied(finalBoardMask, x, y)}></span>
                  {:else}
                    <span class:existing={boardCellOccupied(existingMask, x, y)} class:target={boardCellOccupied(targetMask, x, y)}></span>
                  {/if}
                {/each}
              {/each}
            </div>
          </div>
          <button class="continue-button" type="button" disabled={!canContinue} on:click={continueFromResult}>
            <ArrowDownToLine size={15} strokeWidth={1.8} />{label('useAsNextBase')}
          </button>
        </div>

        <div class="metrics-panel">
          <div class="hero-metric">
            {#if aggregation === 'spin'}
              <span>{label('spinSearchProbability')}</span>
              <strong>{workspaceProbability(language, summary.spin_search_probability)}</strong>
              <small>{number(summaryNumber(summary.spin_search_candidate_count))} {label('spinSearchBuilds')} · {label('spinAccuracy')}: {summary.spin_search_accuracy ?? '—'}{summary.build_mirror_included === 'true' ? ` · ${label('originalAndMirror')}` : ''}</small>
            {:else}
              <span>{label('buildProbability')}</span>
              <strong>{workspaceProbability(language, report?.coverage_probability)}</strong>
              <small>{number(report?.covered_pattern_count)} / {number(report?.materialized_pattern_count)} {label('patterns')}{summary.build_mirror_included === 'true' ? ` · ${label('originalAndMirror')}` : ''}</small>
            {/if}
          </div>
          {#if aggregation === 'spin'}
            <div class="spin-metric">
              <span>{label('buildProbability')}</span>
              <strong>{workspaceProbability(language, report?.coverage_probability)}</strong>
              <small>{number(report?.covered_pattern_count)} / {number(report?.materialized_pattern_count)} {label('patterns')}</small>
            </div>
          {/if}
          <dl>
            <div><dt>{label('buildableTilings')}</dt><dd>{number(report?.unique_solution_count)}</dd></div>
            {#if summary.build_mirror_included === 'true'}
              <div><dt>{label('originalBuildProbability')}</dt><dd>{workspaceProbability(language, summary.original_coverage_probability)}</dd></div>
              <div><dt>{label('mirrorAddedPatterns')}</dt><dd>{number(summaryNumber(summary.mirror_union_added_pattern_count))}</dd></div>
            {/if}
            <div><dt>{label('candidateTilings')}</dt><dd>{number(report?.packing_candidate_count)}</dd></div>
            <div><dt>{label('searchedNodes')}</dt><dd>{number(report?.searched_nodes)}</dd></div>
            <div><dt>{label('memory')}</dt><dd>{bytes(report?.peak_cpu_bytes)}</dd></div>
            <div><dt>{label('probabilityComplete')}</dt><dd>{label(report?.probability_complete ? 'complete' : 'incomplete')}</dd></div>
            <div><dt>{label('countComplete')}</dt><dd>{label(report?.count_complete ? 'complete' : 'incomplete')}</dd></div>
            <div><dt>{label('actualBackend')}</dt><dd>{report?.backend_selected ?? '—'}</dd></div>
            <div><dt>{label('workersUsed')}</dt><dd>{number(report?.workers_used)}</dd></div>
            {#if aggregation === 'spin'}
              <div><dt>{label('executionDistribution')}</dt><dd>{summary.spin_coverage_execution_distribution ?? '—'}</dd></div>
            {/if}
          </dl>
        </div>
      </div>
      <div class="copy-format-row">
        <SolutionCopyFormatControl bind:value={copyFormat} {language} />
      </div>
    {:else if activeTab === 'solutions'}
      {#if solutionKeys.length}
        <SolutionGallery
          {solutionKeys}
          solutionSetHash={report?.normalized_solution_set_hash ?? ''}
          targetLines={height}
          {language}
          {copyFormat}
        />
      {:else}
        <div class="empty-state"><Search size={28} strokeWidth={1.5} /><p>{label('noSolutions')}</p></div>
      {/if}
    {:else}
      {#if view.diagnostics.length || view.error}
        <ul class="diagnostic-list">
          {#each view.diagnostics as diagnostic}
            <li class:error={diagnostic.severity === 'error'}><span>{diagnostic.severity}</span><div><strong>{diagnostic.code}</strong><p>{diagnostic.message}</p></div></li>
          {/each}
          {#if view.error}<li class="error"><span>error</span><div><strong>{label('failed')}</strong><p>{view.error}</p></div></li>{/if}
        </ul>
      {:else}
        <div class="empty-state"><CheckCircle2 size={28} strokeWidth={1.5} /><p>{label('noDiagnostics')}</p></div>
      {/if}
    {/if}
  </ResultWorkspaceFrame>

<style>
  .empty-state { align-items: center; color: #87918d; display: flex; flex-direction: column; justify-content: center; min-height: 220px; text-align: center; }
  .empty-state p { font-size: 13px; margin: 12px 0 0; }
  .result-grid { display: grid; gap: 28px; grid-template-columns: minmax(260px, 430px) minmax(0, 1fr); }
  .copy-format-row { margin-top: 20px; }
  h3 { color: #36423e; font-size: 12px; margin: 0 0 10px; }
  .board-frame { background: #101817; border: 1px solid #253330; border-radius: 6px; padding: 12px; }
  .board { aspect-ratio: calc(10 / var(--board-rows)); display: grid; grid-template-columns: repeat(10, 1fr); grid-template-rows: repeat(var(--board-rows), 1fr); margin: 0 auto; max-height: 520px; max-width: 100%; }
  .board span { background: #1e2927; box-shadow: inset 0 0 0 1px rgba(216, 226, 222, .2); }
  .board span.existing { background: #737d79; box-shadow: inset 2px 2px 0 rgba(255,255,255,.1), inset -2px -2px 0 rgba(20,26,24,.25); }
  .board span.target { background: #d8e2de; box-shadow: inset 2px 2px 0 rgba(255,255,255,.16), inset -2px -2px 0 rgba(41,56,51,.18); }
  .continue-button { align-items: center; background: #fff; border: 1px solid #aebbb5; border-radius: 5px; color: #27403a; cursor: pointer; display: inline-flex; font-size: 11px; font-weight: 700; gap: 7px; margin-top: 10px; min-height: 34px; padding: 7px 10px; }
  .continue-button:disabled { cursor: default; opacity: .4; }
  .metrics-panel { min-width: 0; }
  .hero-metric { border-bottom: 1px solid #dce2de; display: grid; gap: 4px; padding: 2px 0 18px; }
  .hero-metric > span { color: #68736f; font-size: 11px; font-weight: 700; text-transform: uppercase; }
  .hero-metric strong { color: #075f58; font-size: 34px; font-weight: 780; }
  .hero-metric small { color: #6b7671; font-size: 11px; }
  .spin-metric { background: #edf6f3; border-bottom: 1px solid #cfe1dc; display: grid; gap: 3px; padding: 13px 10px; }
  .spin-metric > span { color: #4f625c; font-size: 11px; font-weight: 700; }
  .spin-metric strong { color: #075f58; font-size: 22px; }
  .spin-metric small { color: #64726d; font-size: 10px; }
  dl { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); margin: 16px 0 0; }
  dl div { align-items: baseline; border-bottom: 1px solid #e5e9e6; display: flex; gap: 12px; justify-content: space-between; padding: 10px; }
  dt { color: #68736f; font-size: 11px; }
  dd { color: #24312d; font-size: 12px; font-weight: 700; margin: 0; overflow-wrap: anywhere; text-align: right; }
  .diagnostic-list { display: grid; gap: 1px; list-style: none; margin: 0; padding: 0; }
  .diagnostic-list li { align-items: start; background: #f4f6f5; display: grid; gap: 14px; grid-template-columns: 72px minmax(0, 1fr); padding: 12px; }
  .diagnostic-list li > span { color: #8b5c19; font-size: 10px; font-weight: 800; text-transform: uppercase; }
  .diagnostic-list li.error > span { color: #a63d32; }
  .diagnostic-list strong, .diagnostic-list p { font-size: 12px; margin: 0; }
  .diagnostic-list p { color: #68736f; margin-top: 4px; }
  @media (max-width: 780px) { .result-grid { grid-template-columns: 1fr; } dl { grid-template-columns: 1fr; } }
</style>
