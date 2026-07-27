<script lang="ts">
  import { CheckCircle2, Search } from '@lucide/svelte';
  import { createEventDispatcher } from 'svelte';

  import type {
    ClearraSetupFinderReport,
    ClearraSetupHoldCondition
  } from '../wasm/wasmCommandClient';
  import ResultWorkspaceFrame from './ResultWorkspaceFrame.svelte';
  import SolutionCopyButton from './SolutionCopyButton.svelte';
  import SolutionCopyFormatControl from './SolutionCopyFormatControl.svelte';
  import type { SolutionCopyFormat } from './solutionExport';
  import {
    replaySetupCompletionBoard,
    replaySetupPlacementBoard,
    setupFinalBoard
  } from './setupPlacementBoard';
  import type { WorkspaceRuntimeView } from './workspaceRuntime';
  import {
    workspaceMessage,
    workspaceProbability,
    workspaceProgressDetail,
    workspaceProgressLabel,
    type WorkspaceLanguage
  } from './workspaceI18n';
  import {
    setupPathDetailKey,
    type SetupPathDetailRequest,
    type SetupPathDetailState
  } from './setupFinderModel';

  export let view: WorkspaceRuntimeView;
  export let language: WorkspaceLanguage;
  export let elapsedMs = 0;
  export let pathDetails: Record<string, SetupPathDetailState> = {};

  const PAGE_SIZE = 100;
  const PATH_PAGE_SIZE = 100;
  const dispatch = createEventDispatcher<{ loadPaths: SetupPathDetailRequest }>();
  let visibleCount = PAGE_SIZE;
  let visiblePathCounts: Record<string, number> = {};
  let lastIdentity = '';
  let copyFormat: SolutionCopyFormat = 'fumen';

  $: report = view.searchReport?.setup_report ?? null;
  $: identity = report
    ? `${report.search_mode}:${report.remaining_pieces}:${report.queue_based_pieces}:${report.next_cycle_remaining_pieces}:${report.cycle}:${report.hold_conditions.map((value) => value.candidate_count).join(',')}`
    : '';
  $: if (identity !== lastIdentity) {
    lastIdentity = identity;
    visibleCount = PAGE_SIZE;
    visiblePathCounts = {};
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
      pathKey: setupPathDetailKey({
        conditionId: condition.condition_id,
        setupId: candidate.setup_id
      }),
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

  function requestPaths(
    event: Event,
    conditionId: string,
    setupId: string
  ) {
    const details = event.currentTarget as HTMLDetailsElement;
    const detail = { conditionId, setupId };
    const state = pathDetails[setupPathDetailKey(detail)];
    if (details.open && state?.status !== 'loading' && state?.status !== 'complete') {
      dispatch('loadPaths', detail);
    }
  }

  function retryPaths(conditionId: string, setupId: string) {
    dispatch('loadPaths', { conditionId, setupId });
  }

  function visiblePathCount(key: string): number {
    return visiblePathCounts[key] ?? PATH_PAGE_SIZE;
  }

  function showMorePaths(key: string) {
    visiblePathCounts = {
      ...visiblePathCounts,
      [key]: visiblePathCount(key) + PATH_PAGE_SIZE
    };
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
          <span>{label(report.search_mode === 'qb' ? 'setupModeQb' : 'pcCycle')}</span>
          <strong>{report.search_mode === 'qb'
            ? `${report.remaining_pieces} → ${report.queue_based_pieces}`
            : label('cycleNumber', { cycle: report.cycle })}</strong>
          <small>{report.remaining_pieces}</small>
        </div>
        <dl>
          <div><dt>{label('geometryFamilies')}</dt><dd>{report.geometry_family_count}</dd></div>
          <div><dt>{label('partialBuildStates')}</dt><dd>{number(report.partial_build_node_count)}</dd></div>
          <div><dt>{label('setups')}</dt><dd>{number(totalCandidateCount)}</dd></div>
          <div><dt>{label('countComplete')}</dt><dd>{label(report.complete ? 'complete' : 'incomplete')}</dd></div>
          <div><dt>{label('workersUsed')}</dt><dd>{number(view.searchReport?.workers_used)}</dd></div>
          <div>
            <dt>{label('coverageSemantics')}</dt>
            <dd>{label(report.coverage_semantics === 'visible-seven-policy'
              ? 'visibleSevenCoverage'
              : 'oracleCoverage')}</dd>
          </div>
          {#if report.next_cycle_remaining_pieces}
            <div>
              <dt>{label('setupNextCycleRemaining')}</dt>
              <dd>{report.next_cycle_remaining_pieces}</dd>
            </div>
          {/if}
          {#if report.search_mode === 'oracle'}
            <div><dt>{label('postCycleBorrow')}</dt><dd>{label(report.post_cycle_borrow_enabled ? 'enabled' : 'disabled')}</dd></div>
          {/if}
        </dl>
        <div class="condition-table">
          {#each report.hold_conditions as condition}
            <div>
              <span>{condition.pattern_expression}</span>
              <span>{number(condition.pattern_count)} {label('patterns')}</span>
              <b>{number(condition.candidate_count)} {label('setups')}</b>
            </div>
          {/each}
        </div>
        <SolutionCopyFormatControl bind:value={copyFormat} {language} />
      </div>
    {/if}
  {:else if activeTab === 'solutions'}
    {#if renderedGroups.length}
      {#each renderedGroups as condition}
        <section class="condition-group">
          <div class="condition-heading">
            <div>
              <h3>{condition.pattern_expression}</h3>
              <p>{number(condition.pattern_count)} {label('patterns')}</p>
            </div>
            <span>{number(condition.candidate_count)}</span>
          </div>
          <ol class="setup-grid">
            {#each condition.candidates as result}
              <li>
                <div class="setup-card-actions">
                  <SolutionCopyButton
                    page={result.board.page}
                    format={copyFormat}
                    {language}
                  />
                </div>
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
                  <span>{label(report?.coverage_semantics === 'visible-seven-policy'
                    ? 'conditionalCoverageRatio'
                    : 'conditionalPcProbability')}: {workspaceProbability(language, result.candidate.conditional_pc_probability)}</span>
                  <span>{result.candidate.min_locks === result.candidate.max_locks
                    ? label('lockCount', { count: result.candidate.min_locks })
                    : label('lockRange', { min: result.candidate.min_locks, max: result.candidate.max_locks })}</span>
                </div>
                <details
                  class="setup-path"
                  on:toggle={(event) => requestPaths(
                    event,
                    condition.condition_id,
                    result.candidate.setup_id
                  )}
                >
                  <summary>
                    {#if pathDetails[result.pathKey]?.status === 'loading'}
                      {label('loadingBuildSolutions')}
                    {:else if pathDetails[result.pathKey]?.status === 'complete'}
                      {label('allBuildSolutions')} · {number(pathDetails[result.pathKey].paths.length)}
                    {:else}
                      {label('allBuildSolutions')}
                    {/if}
                  </summary>
                  {#if pathDetails[result.pathKey]?.status === 'loading'}
                    <p class="path-status">{label('loadingExactBuildSolutions')}</p>
                  {:else if pathDetails[result.pathKey]?.status === 'failed'}
                    <div class="path-error">
                      <p>{pathDetails[result.pathKey].error ?? label('pathDetailFailed')}</p>
                      <button
                        type="button"
                        on:click|stopPropagation={() => retryPaths(
                          condition.condition_id,
                          result.candidate.setup_id
                        )}
                      >{label('retry')}</button>
                    </div>
                  {:else if pathDetails[result.pathKey]?.status === 'complete'}
                    {#if pathDetails[result.pathKey].paths.length}
                      <div class="solution-paths">
                        {#each pathDetails[result.pathKey].paths.slice(0, visiblePathCount(result.pathKey)) as path, pathIndex}
                          {@const solutionBoard = replaySetupCompletionBoard(
                            result.candidate.board_mask,
                            path
                          )}
                          <section class="solution-path">
                            <div class="solution-path-heading">
                              <h4>{label('buildSolutionNumber', { number: pathIndex + 1 })}</h4>
                              <SolutionCopyButton
                                page={solutionBoard?.page ?? null}
                                format={copyFormat}
                                {language}
                              />
                            </div>
                            {#if solutionBoard}
                              <div
                                class="setup-board solution-board"
                                style={`--rows:${solutionBoard.height};aspect-ratio:${10 / solutionBoard.height}`}
                                role="img"
                                aria-label={label('buildSolutionNumber', { number: pathIndex + 1 })}
                              >
                                {#each solutionBoard.cells as cell}
                                  <span
                                    class:empty={cell === null}
                                    class:existing={cell === 'G'}
                                    class={`piece-${cell ?? 'empty'}`}
                                  ></span>
                                {/each}
                              </div>
                            {/if}
                          </section>
                        {/each}
                      </div>
                      {#if visiblePathCount(result.pathKey) < pathDetails[result.pathKey].paths.length}
                        <button
                          class="path-more"
                          type="button"
                          on:click={() => showMorePaths(result.pathKey)}
                        >
                          {label('showMore', {
                            count: Math.min(
                              PATH_PAGE_SIZE,
                              pathDetails[result.pathKey].paths.length - visiblePathCount(result.pathKey)
                            )
                          })}
                        </button>
                      {/if}
                    {:else}
                      <p class="path-status">{label('noBuildSolutions')}</p>
                    {/if}
                  {:else}
                    <p class="path-status">{label('loadExactBuildSolutions')}</p>
                  {/if}
                </details>
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
  .condition-table > div { align-items: center; background: #f1f4f2; display: grid; font-size: 11px; gap: 12px; grid-template-columns: minmax(160px, 1fr) minmax(120px, .7fr) minmax(90px, .5fr); padding: 10px 12px; }
  .condition-table b { color: #29413b; }
  .condition-table span { color: #68736f; overflow-wrap: anywhere; }
  .condition-table b { text-align: right; }
  .condition-group + .condition-group { border-top: 1px solid #dce2de; margin-top: 24px; padding-top: 20px; }
  .condition-heading { align-items: center; display: flex; justify-content: space-between; margin-bottom: 10px; }
  .condition-heading h3 { color: #26322e; font-size: 13px; margin: 0; }
  .condition-heading p { color: #75807b; font-size: 10px; margin: 4px 0 0; }
  .condition-heading > span { background: #e7eeeb; border-radius: 3px; color: #285049; font-size: 10px; font-weight: 800; padding: 4px 7px; }
  .setup-grid { display: grid; gap: 12px; grid-template-columns: repeat(auto-fill, minmax(180px, 1fr)); list-style: none; margin: 0; padding: 0; }
  .setup-grid li { background: #f3f5f4; border: 1px solid #d7ded9; border-radius: 6px; min-width: 0; padding: 10px; }
  .setup-card-actions { display: flex; height: 28px; justify-content: flex-end; margin-bottom: 4px; }
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
  .path-status { color: #68736f; font-size: 10px; margin: 8px 0 0; }
  .path-error { align-items: start; border-left: 2px solid #b95449; display: grid; gap: 7px; margin-top: 8px; padding-left: 8px; }
  .path-error p { color: #8b3e36; font-size: 10px; margin: 0; overflow-wrap: anywhere; }
  .path-error button, .path-more { background: #fff; border: 1px solid #aebbb6; border-radius: 4px; color: #174a45; cursor: pointer; font-size: 10px; font-weight: 750; min-height: 28px; padding: 0 9px; width: fit-content; }
  .solution-paths { display: grid; gap: 8px; margin-top: 8px; }
  .solution-path { border-top: 1px solid #d8dfdb; padding-top: 6px; }
  .solution-path:first-child { border-top: 0; padding-top: 0; }
  .solution-path-heading { align-items: center; display: flex; justify-content: space-between; margin-bottom: 4px; }
  .solution-path h4 { color: #37534d; font-size: 9px; margin: 0; }
  .solution-board { width: 100%; }
  .path-more { margin-top: 8px; }
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
