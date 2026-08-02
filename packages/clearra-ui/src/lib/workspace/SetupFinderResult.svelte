<script lang="ts">
  import { Search } from '@lucide/svelte';
  import { createEventDispatcher } from 'svelte';

  import type {
    ClearraSetupCandidate,
    ClearraSetupFinderReport,
    ClearraSetupHoldCondition
  } from '../wasm/wasmCommandClient';
  import ResultWorkspaceFrame from './ResultWorkspaceFrame.svelte';
  import SolutionCopyButton from './SolutionCopyButton.svelte';
  import SolutionCopyFormatControl from './SolutionCopyFormatControl.svelte';
  import type {
    SolutionCopyFormat,
    SolutionExportPage
  } from './solutionExport';
  import {
    replaySetupCompletionBoard,
    replaySetupPlacementBoard,
    setupFinalBoard,
    type SetupPlacementBoard
  } from './setupPlacementBoard';
  import type { WorkspaceRuntimeView } from './workspaceRuntime';
  import {
    workspaceMessage,
    workspaceProbability,
    workspaceProgressLabel,
    type WorkspaceLanguage
  } from './workspaceI18n';
  import {
    setupPathDetailKey,
    type SetupSearchMode,
    type SetupPathDetailRequest,
    type SetupPathDetailState
  } from './setupFinderModel';

  export let view: WorkspaceRuntimeView;
  export let language: WorkspaceLanguage;
  export let elapsedMs = 0;
  export let searchMode: SetupSearchMode = 'oracle';
  export let pathDetails: Record<string, SetupPathDetailState> = {};

  const PAGE_SIZE = 100;
  const dispatch = createEventDispatcher<{ loadPaths: SetupPathDetailRequest }>();
  let visibleCandidateCount = PAGE_SIZE;
  let visiblePathCounts: Record<string, number> = {};
  let lastReport: ClearraSetupFinderReport | null = null;
  let copyFormat: SolutionCopyFormat = 'ctk';
  const setupBoardCache = new Map<string, {
    candidate: ClearraSetupCandidate;
    board: SetupPlacementBoard;
  }>();
  const pathBoardCache = new Map<string, {
    paths: SetupPathDetailState['paths'];
    setupMask: string;
    boards: Array<SetupPlacementBoard | null>;
  }>();

  $: report = view.searchReport?.setup_report ?? null;
  $: if (report !== lastReport) {
    lastReport = report;
    visibleCandidateCount = PAGE_SIZE;
    visiblePathCounts = {};
    setupBoardCache.clear();
    pathBoardCache.clear();
  }
  $: candidateEntries = setupCandidateEntries(report);
  $: retainedCandidateCount = candidateEntries.length;
  $: totalCandidateCount = report?.hold_conditions.reduce(
    (total, condition) => total + condition.candidate_count,
    0
  ) ?? 0;
  $: visibleCandidateCount = Math.min(visibleCandidateCount, retainedCandidateCount);
  $: preparedCandidateCount = Math.min(
    retainedCandidateCount,
    visibleCandidateCount + PAGE_SIZE
  );
  $: preparedCandidates = prepareSetupCandidates(
    candidateEntries,
    0,
    preparedCandidateCount
  );
  $: renderedGroups = groupSetupCandidates(
    preparedCandidates.slice(0, visibleCandidateCount)
  );
  $: label = (
    key: Parameters<typeof workspaceMessage>[1],
    values: Record<string, string | number> = {}
  ) => workspaceMessage(language, key, values);

  function number(value: number | undefined): string {
    return value === undefined ? '—' : new Intl.NumberFormat(language).format(value);
  }

  type SetupCandidateEntry = {
    condition: ClearraSetupHoldCondition;
    candidate: ClearraSetupCandidate;
    pathKey: string;
  };

  type PreparedSetupCandidate = SetupCandidateEntry & {
    board: SetupPlacementBoard;
  };

  type PreparedSetupGroup = Omit<ClearraSetupHoldCondition, 'candidates'> & {
    candidates: PreparedSetupCandidate[];
  };

  type PreparedPathItem = {
    index: number;
    board: SetupPlacementBoard | null;
  };

  type PreparedPathWindow = {
    visible: number;
    total: number;
    items: PreparedPathItem[];
  };

  function setupCandidateEntries(
    setup: ClearraSetupFinderReport | null
  ): SetupCandidateEntry[] {
    if (!setup) return [];
    const entries: SetupCandidateEntry[] = [];
    for (const condition of setup.hold_conditions) {
      for (const candidate of condition.candidates) {
        entries.push({
          condition,
          candidate,
          pathKey: setupPathDetailKey({
            conditionId: condition.condition_id,
            setupId: candidate.setup_id
          })
        });
      }
    }
    return entries;
  }

  function prepareSetupCandidates(
    entries: SetupCandidateEntry[],
    start: number,
    end: number
  ): PreparedSetupCandidate[] {
    return entries.slice(start, end).map((entry) => {
      const cached = setupBoardCache.get(entry.pathKey);
      if (cached?.candidate === entry.candidate) {
        return { ...entry, board: cached.board };
      }
      const board = replaySetupPlacementBoard(
        entry.candidate.board_mask,
        entry.candidate.representative_path
      ) ?? setupFinalBoard(entry.candidate.board_mask);
      setupBoardCache.set(entry.pathKey, { candidate: entry.candidate, board });
      return { ...entry, board };
    });
  }

  function groupSetupCandidates(
    entries: PreparedSetupCandidate[]
  ): PreparedSetupGroup[] {
    const groups: PreparedSetupGroup[] = [];
    const byCondition = new Map<string, PreparedSetupGroup>();
    for (const entry of entries) {
      let group = byCondition.get(entry.condition.condition_id);
      if (!group) {
        group = { ...entry.condition, candidates: [] };
        byCondition.set(entry.condition.condition_id, group);
        groups.push(group);
      }
      group.candidates.push(entry);
    }
    return groups;
  }

  function showMoreCandidates() {
    visibleCandidateCount = Math.min(
      retainedCandidateCount,
      visibleCandidateCount + PAGE_SIZE
    );
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

  function preparePathWindow(key: string, setupMask: string): PreparedPathWindow {
    const paths = pathDetails[key]?.paths ?? [];
    const total = paths.length;
    const visible = Math.min(total, visiblePathCounts[key] ?? PAGE_SIZE);
    const preparedEnd = Math.min(total, visible + PAGE_SIZE);
    let cache = pathBoardCache.get(key);
    if (!cache || cache.paths !== paths || cache.setupMask !== setupMask) {
      cache = { paths, setupMask, boards: [] };
      pathBoardCache.set(key, cache);
    }
    for (let index = 0; index < preparedEnd; index += 1) {
      if (!(index in cache.boards)) {
        cache.boards[index] = replaySetupCompletionBoard(setupMask, paths[index]);
      }
    }
    const items: PreparedPathItem[] = [];
    for (let index = 0; index < visible; index += 1) {
      items.push({ index, board: cache.boards[index] ?? null });
    }
    return { visible, total, items };
  }

  function showMorePaths(key: string, total: number) {
    visiblePathCounts = {
      ...visiblePathCounts,
      [key]: Math.min(total, (visiblePathCounts[key] ?? PAGE_SIZE) + PAGE_SIZE)
    };
  }

  async function loadAllSetupPages(
    signal?: AbortSignal
  ): Promise<SolutionExportPage[]> {
    const pages: SolutionExportPage[] = [];
    for (let offset = 0; offset < candidateEntries.length; offset += PAGE_SIZE) {
      throwIfAborted(signal);
      const end = Math.min(candidateEntries.length, offset + PAGE_SIZE);
      for (let index = offset; index < end; index += 1) {
        const entry = candidateEntries[index];
        const board =
          replaySetupPlacementBoard(
            entry.candidate.board_mask,
            entry.candidate.representative_path
          ) ?? setupFinalBoard(entry.candidate.board_mask);
        if (!board.page) throw new Error('Setup page could not be reconstructed.');
        pages.push(board.page);
      }
      if (end < candidateEntries.length) await nextPaint();
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
  ariaLabel={label('setupResults')}
  status={view.status}
  statusLabel={label(view.status)}
  elapsedLabel={label('elapsed')}
  elapsedText={`${(elapsedMs / 1000).toFixed(1)}s`}
  progressProfile="setup"
  progressMode={searchMode === 'qb' ? 'setup-qb' : 'setup-oracle'}
  {language}
  progressLabel={(workspaceProgressLabel(language, view.progressTelemetry) ?? view.progressLabel) || label('idle')}
  progressDetail=""
  progressDone={view.progressDone}
  progressTotal={view.progressTotal}
  progressTelemetry={view.progressTelemetry}
  failureDiagnostics={view.diagnostics}
  failureMessage={view.error ?? ''}
>
  {#if view.status === 'idle' && !report}
    <div class="empty-state"><Search size={28} strokeWidth={1.5} /><p>{label('noSetupResult')}</p></div>
  {:else if report && view.status !== 'failed' && view.status !== 'terminated'}
    <div class="setup-content">
      <section class="setup-overview" aria-label={label('overview')}>
        <div class="overview-lead">
          <span>{label(report.search_mode === 'qb' ? 'setupModeQb' : 'pcCycle')}</span>
          <strong>{report.search_mode === 'qb'
            ? `${report.remaining_pieces} → ${report.queue_based_pieces}`
            : label('cycleNumber', { cycle: report.cycle })}</strong>
          <small>{report.remaining_pieces}</small>
        </div>
        <dl>
          <div><dt>{label('setups')}</dt><dd>{number(totalCandidateCount)}</dd></div>
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
        <SolutionCopyFormatControl
          bind:value={copyFormat}
          {language}
          loadPages={candidateEntries.length ? loadAllSetupPages : null}
        />
      </section>

      <section
        class="setup-solutions"
        aria-label={label('setups')}
      >
        <div class="solutions-heading">
          <h2>{label('setups')}</h2>
          <span>{number(retainedCandidateCount)}</span>
        </div>
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
                      <span>{label(report.coverage_semantics === 'visible-seven-policy'
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
                        {@const pathWindow = preparePathWindow(
                          result.pathKey,
                          result.candidate.board_mask
                        )}
                        {#if pathWindow.items.length}
                          <div class="solution-paths">
                            {#each pathWindow.items as solution}
                              <section class="solution-path">
                                <div class="solution-path-heading">
                                  <h4>{label('buildSolutionNumber', { number: solution.index + 1 })}</h4>
                                  <SolutionCopyButton
                                    page={solution.board?.page ?? null}
                                    format={copyFormat}
                                    {language}
                                  />
                                </div>
                                {#if solution.board}
                                  <div
                                    class="setup-board solution-board"
                                    style={`--rows:${solution.board.height};aspect-ratio:${10 / solution.board.height}`}
                                    role="img"
                                    aria-label={label('buildSolutionNumber', { number: solution.index + 1 })}
                                  >
                                    {#each solution.board.cells as cell}
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
                          {#if pathWindow.visible < pathWindow.total}
                            <button
                              class="more"
                              type="button"
                              on:click={() => showMorePaths(result.pathKey, pathWindow.total)}
                            >{label('showMore', {
                              count: Math.min(PAGE_SIZE, pathWindow.total - pathWindow.visible)
                            })}</button>
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
          {#if visibleCandidateCount < retainedCandidateCount}
            <button class="more" type="button" on:click={showMoreCandidates}>
              {label('showMore', {
                count: Math.min(PAGE_SIZE, retainedCandidateCount - visibleCandidateCount)
              })}
            </button>
          {/if}
        {:else}
          <div class="empty-state"><Search size={28} strokeWidth={1.5} /><p>{label('noSetups')}</p></div>
        {/if}
      </section>
    </div>
  {/if}
</ResultWorkspaceFrame>

<style>
  .empty-state { align-items: center; color: #87918d; display: flex; flex-direction: column; justify-content: center; min-height: 220px; text-align: center; }
  .empty-state p { font-size: 13px; margin: 12px 0 0; }
  .setup-content { display: grid; gap: 32px; }
  .setup-overview { border-bottom: 1px solid #dce2de; display: grid; gap: 18px; padding-bottom: 28px; }
  .overview-lead { border-bottom: 1px solid #dce2de; display: grid; gap: 4px; padding-bottom: 16px; }
  .overview-lead > span { color: #68736f; font-size: 11px; font-weight: 700; text-transform: uppercase; }
  .overview-lead strong { color: #075f58; font-size: 24px; }
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
  .setup-solutions { scroll-margin-top: 18px; }
  .solutions-heading { align-items: center; display: flex; justify-content: space-between; margin-bottom: 18px; }
  .solutions-heading h2 { color: #26322e; font-size: 15px; margin: 0; }
  .solutions-heading span { background: #e7eeeb; border-radius: 3px; color: #285049; font-size: 11px; font-weight: 800; padding: 5px 8px; }
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
  .path-error button { background: #fff; border: 1px solid #aebbb6; border-radius: 4px; color: #174a45; cursor: pointer; font-size: 10px; font-weight: 750; min-height: 28px; padding: 0 9px; width: fit-content; }
  .solution-paths { display: grid; gap: 8px; margin-top: 8px; }
  .solution-path { border-top: 1px solid #d8dfdb; padding-top: 6px; }
  .solution-path:first-child { border-top: 0; padding-top: 0; }
  .solution-path-heading { align-items: center; display: flex; justify-content: space-between; margin-bottom: 4px; }
  .solution-path h4 { color: #37534d; font-size: 9px; margin: 0; }
  .solution-board { width: 100%; }
  .more { background: #fff; border: 1px solid #aebbb6; border-radius: 5px; color: #174a45; cursor: pointer; display: block; font-size: 11px; font-weight: 750; margin: 18px auto 0; min-height: 34px; padding: 0 14px; }
  .setup-path .more { margin-top: 9px; min-height: 30px; }
  @media (max-width: 700px) {
    dl, .condition-table > div { grid-template-columns: 1fr; }
    .condition-table b { text-align: left; }
  }
</style>
