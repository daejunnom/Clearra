<script lang="ts">
  import { CheckCircle2, Search } from '@lucide/svelte';

  import type {
    ClearraSetupFinderReport,
    ClearraSetupHoldCondition
  } from '../wasm/wasmCommandClient';
  import ResultWorkspaceFrame from './ResultWorkspaceFrame.svelte';
  import { replaySetupPlacementBoard, setupFinalBoard } from './setupPlacementBoard';
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

  const PAGE_SIZE = 100;
  let visibleCount = PAGE_SIZE;
  let lastIdentity = '';

  $: report = view.searchReport?.setup_report ?? null;
  $: identity = report
    ? `${report.remaining_pieces}:${report.cycle}:${report.hold_conditions.map((value) => value.candidate_count).join(',')}`
    : '';
  $: if (identity !== lastIdentity) {
    lastIdentity = identity;
    visibleCount = PAGE_SIZE;
  }
  $: retainedCandidateCount = report?.hold_conditions.reduce(
    (total, condition) => total + condition.candidates.length,
    0
  ) ?? 0;
  $: totalCandidateCount = report?.hold_conditions.reduce(
    (total, condition) => total + condition.candidate_count,
    0
  ) ?? 0;
  $: visibleGroups = visibleSetupGroups(report, visibleCount);
  $: renderedGroups = visibleGroups.map((condition) => ({
    ...condition,
    candidates: condition.candidates.map((candidate) => ({
      candidate,
      board: replaySetupPlacementBoard(
        candidate.board_mask,
        candidate.representative_path
      ) ?? setupFinalBoard(candidate.board_mask)
    }))
  }));
  $: visibleCandidateCount = visibleGroups.reduce(
    (total, group) => total + group.candidates.length,
    0
  );
  $: remainingCandidates = Math.max(0, retainedCandidateCount - visibleCandidateCount);
  $: hasOutput = Boolean(view.response || report || view.diagnostics.length || view.error);
  $: label = (
    key: Parameters<typeof workspaceMessage>[1],
    values: Record<string, string | number> = {}
  ) => workspaceMessage(language, key, values);

  function number(value: number | undefined): string {
    return value === undefined ? '—' : new Intl.NumberFormat(language).format(value);
  }

  function holdLabel(condition: ClearraSetupHoldCondition): string {
    return condition.initial_hold ?? label('empty');
  }

  function holdActionLabel(action: string): string {
    if (action === 'swap-held') return label('swapHeld');
    if (action === 'store-current-use-next') return label('storeCurrentUseNext');
    if (action === 'use-held-terminal') return label('useHeldTerminal');
    return label('useCurrent');
  }

  function visibleSetupGroups(
    setup: ClearraSetupFinderReport | null,
    limit: number
  ): ClearraSetupHoldCondition[] {
    if (!setup) return [];
    let remaining = limit;
    const groups: ClearraSetupHoldCondition[] = [];
    for (const condition of setup.hold_conditions) {
      if (remaining <= 0) break;
      const candidates = condition.candidates.slice(0, remaining);
      if (candidates.length) groups.push({ ...condition, candidates });
      remaining -= candidates.length;
    }
    return groups;
  }

</script>

<ResultWorkspaceFrame
  ariaLabel={label('setupResults')}
  status={view.status}
  statusLabel={label(view.status)}
  elapsedLabel={label('elapsed')}
  elapsedText={`${(elapsedMs / 1000).toFixed(1)}s`}
  runtimeTitle={label('runtime')}
  runtimeLabel={label('runtimeWeb')}
  progressAriaLabel={label('progress')}
  progressLabel={(workspaceProgressLabel(language, view.progressTelemetry) ?? view.progressLabel) || label('idle')}
  progressDetail={workspaceProgressDetail(language, view.progressTelemetry)}
  progressDone={view.progressDone}
  progressTotal={view.progressTotal}
  progressDoneText={number(view.progressDone)}
  progressTotalText={number(view.progressTotal)}
  overviewLabel={label('overview')}
  solutionsLabel={label('setups')}
  solutionCountText={number(totalCandidateCount)}
  diagnosticsLabel={label('diagnostics')}
  diagnosticCountText={number(view.diagnostics.length)}
  let:activeTab
>
  {#if !hasOutput && view.status === 'idle'}
    <div class="empty-state"><Search size={28} strokeWidth={1.5} /><p>{label('noSetupResult')}</p></div>
  {:else if activeTab === 'overview'}
    {#if report}
      <div class="setup-overview">
        <div class="overview-lead">
          <span>{label('pcCycle')}</span>
          <strong>{label('cycleNumber', { cycle: report.cycle })}</strong>
          <small>{report.remaining_pieces} · {number(report.hold_conditions.length)} {label('holdConditions')}</small>
        </div>
        <dl>
          <div><dt>{label('geometryFamilies')}</dt><dd>{report.geometry_family_count}</dd></div>
          <div><dt>{label('partialBuildStates')}</dt><dd>{number(report.partial_build_node_count)}</dd></div>
          <div><dt>{label('setups')}</dt><dd>{number(totalCandidateCount)}</dd></div>
          <div><dt>{label('countComplete')}</dt><dd>{label(report.complete ? 'complete' : 'incomplete')}</dd></div>
          <div><dt>{label('workersUsed')}</dt><dd>{number(view.searchReport?.workers_used)}</dd></div>
          <div><dt>{label('coverageSemantics')}</dt><dd>{label('oracleCoverage')}</dd></div>
          <div><dt>{label('postCycleBorrow')}</dt><dd>{label(report.post_cycle_borrow_enabled ? 'enabled' : 'disabled')}</dd></div>
        </dl>
        <div class="condition-table">
          {#each report.hold_conditions as condition}
            <div>
              <strong>{label('initialHold')}: {holdLabel(condition)}</strong>
              <span>{condition.pattern_expression}</span>
              <span>{number(condition.pattern_count)} {label('patterns')}</span>
              <b>{number(condition.candidate_count)} {label('setups')}</b>
            </div>
          {/each}
        </div>
      </div>
    {/if}
  {:else if activeTab === 'solutions'}
    {#if renderedGroups.length}
      {#each renderedGroups as condition}
        <section class="condition-group">
          <div class="condition-heading">
            <div>
              <h3>{label('initialHold')}: {holdLabel(condition)}</h3>
              <p>{condition.pattern_expression} · {number(condition.pattern_count)} {label('patterns')}</p>
            </div>
            <span>{number(condition.candidate_count)}</span>
          </div>
          <ol class="setup-grid">
            {#each condition.candidates as result}
              <li>
                <div
                  class="setup-board"
                  style={`--rows:${result.board.height};aspect-ratio:${10 / result.board.height}`}
                  role="img"
                  aria-label={result.candidate.setup_id}
                >
                  {#each result.board.cells as cell}
                    <span
                      class:empty={cell === null}
                      class:existing={cell === 'G'}
                      class={`piece-${cell ?? 'empty'}`}
                    ></span>
                  {/each}
                </div>
                <div class="setup-metrics">
                  <strong>{label('jointProbability')}: {workspaceProbability(language, result.candidate.joint_probability)}</strong>
                  <span>{label('buildProbability')}: {workspaceProbability(language, result.candidate.build_probability)}</span>
                  <span>{label('conditionalPcProbability')}: {workspaceProbability(language, result.candidate.conditional_pc_probability)}</span>
                  <span>{result.candidate.min_locks === result.candidate.max_locks
                    ? label('lockCount', { count: result.candidate.min_locks })
                    : label('lockRange', { min: result.candidate.min_locks, max: result.candidate.max_locks })}</span>
                </div>
                {#if result.candidate.representative_path.length}
                  <details class="setup-path">
                    <summary>{label('representativeBuild')}</summary>
                    <ol>
                      {#each result.candidate.representative_path as step}
                        <li>
                          <b>{step.piece}</b>
                          <span>R{step.rotation} · ({step.x}, {step.y})</span>
                          <span>{holdActionLabel(step.hold)}</span>
                          {#if step.cleared_lines > 0}
                            <span>{label('clearedLineCount', { count: step.cleared_lines })}</span>
                          {/if}
                        </li>
                      {/each}
                    </ol>
                  </details>
                {/if}
              </li>
            {/each}
          </ol>
        </section>
      {/each}
      {#if remainingCandidates > 0}
        <div class="load-more-row">
          <button type="button" on:click={() => (visibleCount += PAGE_SIZE)}>
            {label('showMore', { count: Math.min(PAGE_SIZE, remainingCandidates) })}
          </button>
        </div>
      {/if}
    {:else}
      <div class="empty-state"><Search size={28} strokeWidth={1.5} /><p>{label('noSetups')}</p></div>
    {/if}
  {:else}
    {#if view.diagnostics.length || view.error}
      <ul class="diagnostic-list">
        {#each view.diagnostics as diagnostic}
          <li class:error={diagnostic.severity === 'error'}>
            <span>{diagnostic.severity}</span>
            <div><strong>{diagnostic.code}</strong><p>{diagnostic.message}</p></div>
          </li>
        {/each}
        {#if view.error}
          <li class="error"><span>error</span><div><strong>{label('failed')}</strong><p>{view.error}</p></div></li>
        {/if}
      </ul>
    {:else}
      <div class="empty-state"><CheckCircle2 size={28} strokeWidth={1.5} /><p>{label('noDiagnostics')}</p></div>
    {/if}
  {/if}
</ResultWorkspaceFrame>

<style>
  .empty-state { align-items: center; color: #87918d; display: flex; flex-direction: column; justify-content: center; min-height: 220px; text-align: center; }
  .empty-state p { font-size: 13px; margin: 12px 0 0; }
  .setup-overview { display: grid; gap: 18px; }
  .overview-lead { border-bottom: 1px solid #dce2de; display: grid; gap: 4px; padding-bottom: 16px; }
  .overview-lead > span { color: #68736f; font-size: 11px; font-weight: 700; text-transform: uppercase; }
  .overview-lead strong { color: #075f58; font-size: 30px; }
  .overview-lead small { color: #6b7671; font-size: 11px; }
  dl { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); margin: 0; }
  dl div { align-items: baseline; border-bottom: 1px solid #e5e9e6; display: flex; gap: 12px; justify-content: space-between; padding: 10px; }
  dt { color: #68736f; font-size: 11px; }
  dd { color: #24312d; font-size: 12px; font-weight: 700; margin: 0; text-align: right; }
  .condition-table { display: grid; gap: 1px; }
  .condition-table > div { align-items: center; background: #f1f4f2; display: grid; font-size: 11px; gap: 12px; grid-template-columns: minmax(120px, .8fr) minmax(150px, 1.2fr) minmax(100px, .6fr) minmax(90px, .5fr); padding: 10px 12px; }
  .condition-table strong, .condition-table b { color: #29413b; }
  .condition-table span { color: #68736f; overflow-wrap: anywhere; }
  .condition-table b { text-align: right; }
  .condition-group + .condition-group { border-top: 1px solid #dce2de; margin-top: 24px; padding-top: 20px; }
  .condition-heading { align-items: center; display: flex; justify-content: space-between; margin-bottom: 10px; }
  .condition-heading h3 { color: #26322e; font-size: 13px; margin: 0; }
  .condition-heading p { color: #75807b; font-size: 10px; margin: 4px 0 0; }
  .condition-heading > span { background: #e7eeeb; border-radius: 3px; color: #285049; font-size: 10px; font-weight: 800; padding: 4px 7px; }
  .setup-grid { display: grid; gap: 12px; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); list-style: none; margin: 0; padding: 0; }
  .setup-grid li { background: #f3f5f4; border: 1px solid #d7ded9; border-radius: 6px; min-width: 0; padding: 10px; }
  .setup-board { background: #101817; border: 1px solid #253330; border-radius: 4px; display: grid; gap: 0; grid-template-columns: repeat(10, minmax(0, 1fr)); grid-template-rows: repeat(var(--rows), minmax(0, 1fr)); overflow: hidden; }
  .setup-board span { background: var(--cell-color); box-shadow: 0 0 0 .5px var(--cell-color); min-height: 0; min-width: 0; }
  .setup-board span.empty { --cell-color: #1e2927; box-shadow: inset 0 0 0 1px rgba(216, 226, 222, .18); }
  .setup-board span.existing { --cell-color: #d8e2de; }
  .piece-I { --cell-color: #60d6db; }
  .piece-O { --cell-color: #f2cb52; }
  .piece-T { --cell-color: #c47bdc; }
  .piece-S { --cell-color: #70c982; }
  .piece-Z { --cell-color: #ec7771; }
  .piece-J { --cell-color: #6d91e5; }
  .piece-L { --cell-color: #eaa05d; }
  .setup-metrics { display: grid; gap: 3px; margin-top: 8px; }
  .setup-metrics strong { color: #075f58; font-size: 11px; }
  .setup-metrics span { color: #697570; font-size: 10px; }
  .setup-path { border-top: 1px solid #d8dfdb; margin-top: 8px; padding-top: 7px; }
  .setup-path summary { color: #37534d; cursor: pointer; font-size: 10px; font-weight: 750; }
  .setup-path ol { display: grid; gap: 3px; list-style-position: inside; margin: 7px 0 0; padding: 0; }
  .setup-path li { align-items: baseline; background: #e9eeeb; border: 0; display: grid; font-size: 9px; gap: 4px; grid-template-columns: 18px minmax(70px, .8fr) minmax(90px, 1fr); padding: 4px 6px; }
  .setup-path li b { color: #154c46; }
  .setup-path li span { color: #64706b; }
  .load-more-row { display: flex; justify-content: center; padding-top: 18px; }
  .load-more-row button { background: #fff; border: 1px solid #aebbb6; border-radius: 5px; color: #174a45; cursor: pointer; font-size: 12px; font-weight: 750; min-height: 36px; padding: 0 16px; }
  .diagnostic-list { display: grid; gap: 1px; list-style: none; margin: 0; padding: 0; }
  .diagnostic-list li { align-items: start; background: #f4f6f5; display: grid; gap: 14px; grid-template-columns: 72px minmax(0, 1fr); padding: 12px; }
  .diagnostic-list li > span { color: #8b5c19; font-size: 10px; font-weight: 800; text-transform: uppercase; }
  .diagnostic-list li.error > span { color: #a63d32; }
  .diagnostic-list strong, .diagnostic-list p { font-size: 12px; margin: 0; }
  .diagnostic-list p { color: #68736f; margin-top: 4px; }
  @media (max-width: 700px) {
    dl, .condition-table > div { grid-template-columns: 1fr; }
    .condition-table b { text-align: left; }
  }
</style>
