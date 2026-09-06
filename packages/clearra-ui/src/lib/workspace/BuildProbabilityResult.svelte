<script lang="ts">
  import { ArrowDownToLine, Check, Copy, Search } from '@lucide/svelte';
  import { createEventDispatcher } from 'svelte';

  import ResultWorkspaceFrame from './ResultWorkspaceFrame.svelte';
  import ProductResultPager from './ProductResultPager.svelte';
  import SolutionCopyFormatControl from './SolutionCopyFormatControl.svelte';
  import SolutionGallery from './SolutionGallery.svelte';
  import type { SolutionCopyFormat } from './solutionExport';
  import { writeClipboardText } from './clipboardText';
  import {
    workspaceSolutionCount,
    workspaceSolutionKeysComplete,
    workspaceSolutionPageAvailable
  } from './solutionSetAvailability';
  import {
    bindSolutionPageLoader,
    createPagedSolutionExportKeySource,
    solutionPageResultIdentity,
    type SolutionPageLoader
  } from './solutionPageSource';
  import { boardCellOccupied } from './solverWorkspaceModel';
  import { BUILD_PROBABILITY_PRIMARY_METRIC } from './buildProbabilityModel';
  import {
    buildCoveragePortfolioSummary,
    buildProbabilityAggregationAuthority,
    buildProbabilityCoverageAggregation
  } from './buildProbabilityAggregation';
  import {
    buildProbabilityFinesseView,
    formatFinesseInputCount
  } from './buildProbabilityFinesse';
  import type { ClearraFinessePolicyResult } from '../wasm/wasmCommandClient';
  import type { WorkspaceRuntimeView } from './workspaceRuntime';
  import type {
    ProductMemberPageLoader,
    ProductNextPageLoader,
    ProductPageRelease
  } from './productResultPager';
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
  export let aggregation: 'buildability' | 'tiling' | 'spin' = 'buildability';
  export let resultMode:
    | 'all-solutions'
    | 'complete-replay-paths'
    | 'minimum-solutions'
    | 'field-average-score'
    | 'fixed-queue-maximum-score'
    | 'highest-score-minimum-set'
    | 'failed-queues' = 'all-solutions';
  export let loadSolutionPage: SolutionPageLoader | null = null;
  export let loadNextProductPage: ProductNextPageLoader | null = null;
  export let loadProductMemberPage: ProductMemberPageLoader | null = null;
  export let releaseProductPages: ProductPageRelease | null = null;

  const dispatch = createEventDispatcher<{ continue: { existingMask: bigint; height: number } }>();
  const columns = Array.from({ length: 10 }, (_, index) => index);
  let copyFormat: SolutionCopyFormat = 'ctk';
  let failedQueueCopyComplete = false;
  let visibleFailedQueueCount = 100;
  let failedResultIdentity = '';

  $: rows = Array.from({ length: height }, (_, index) => height - index - 1);
  $: report = view.searchReport;
  $: productResultPayload = view.response?.product_result_payload ?? null;
  $: buildCoverSummary = buildCoveragePortfolioSummary(productResultPayload);
  $: buildScoreFieldSummary = productResultPayload?.content.payload_kind === 'pc-score-field-summary'
    ? productResultPayload.content.payload
    : null;
  $: aggregationAuthority = buildProbabilityAggregationAuthority(report, aggregation);
  $: effectiveAggregation = aggregationAuthority.effective ?? aggregation;
  $: coverageAggregation = buildProbabilityCoverageAggregation(report, effectiveAggregation);
  $: authorizedCoverage = coverageAggregation.state === 'authorized'
    ? coverageAggregation
    : null;
  $: solutionCount = buildScoreFieldSummary
    ? buildScoreFieldSummary.fields.length
    : (buildCoverSummary?.selectedCandidateCount ?? workspaceSolutionCount(report));
  $: summary = Object.fromEntries(report?.summary_fields ?? []);
  $: failedQueueResult = resultMode === 'failed-queues' &&
    summary.build_failed_queue_contract === 'exact-build-coverage-complement.v1';
  $: failedQueueEntries = failedQueueResult
    ? Object.entries(summary)
        .filter(([key]) => /^failed_pattern_[0-9]+$/u.test(key))
        .sort(([left], [right]) => failedQueueIndex(left) - failedQueueIndex(right))
        .map(([, queue]) => queue)
    : [];
  $: nextFailedQueueCount = Math.min(
    100,
    Math.max(0, failedQueueEntries.length - visibleFailedQueueCount)
  );
  $: nextFailedResultIdentity = failedQueueResult
    ? `${report?.normalized_solution_set_hash ?? ''}:${summary.total_pattern_count ?? ''}:${summary.failed_pattern_count ?? ''}:${summary.failed_pattern_examples_materialized ?? ''}`
    : '';
  $: if (nextFailedResultIdentity !== failedResultIdentity) {
    failedResultIdentity = nextFailedResultIdentity;
    visibleFailedQueueCount = 100;
    failedQueueCopyComplete = false;
  }
  $: solutionProbabilityByKey = Object.fromEntries(
    (report?.solution_probabilities ?? []).map((entry) => [entry.solution_key, entry])
  );
  $: finesseView = buildProbabilityFinesseView(report?.finesse_report);
  $: solutionKeys = buildScoreFieldSummary
    ? buildScoreFieldSummary.fields.map((field) => field.normalized_field_key)
    : (report?.normalized_solution_keys ?? []);
  $: solutionAverageScoreByKey = buildScoreFieldSummary
    ? Object.fromEntries(
        buildScoreFieldSummary.fields.map((field) => [field.normalized_field_key, field])
      )
    : Object.fromEntries(
        (report?.solution_average_scores ?? []).map((field) => [field.solution_key, field])
      );
  $: solutionKeysComplete = buildScoreFieldSummary
    ? true
    : workspaceSolutionKeysComplete(report);
  $: solutionPageAvailable = buildScoreFieldSummary
    ? false
    : workspaceSolutionPageAvailable(report);
  $: solutionResultIdentity = solutionPageResultIdentity(
    report?.normalized_solution_set_hash,
    solutionCount,
    solutionKeys
  );
  $: boundSolutionPageLoader =
    solutionCount !== null &&
    solutionCount > 0 &&
    solutionPageAvailable &&
    loadSolutionPage
      ? bindSolutionPageLoader({
          keyCount: solutionCount,
          loadPage: loadSolutionPage,
          resultIdentity: solutionResultIdentity,
          currentResultIdentity: () => solutionPageResultIdentity(
            report?.normalized_solution_set_hash,
            workspaceSolutionCount(report),
            report?.normalized_solution_keys ?? []
          )
        })
      : null;
  $: solutionCommentByKey = buildSolutionComments();
  $: solutionExportKeySource = boundSolutionPageLoader && solutionCount !== null
    ? createPagedSolutionExportKeySource({
        keyCount: solutionCount,
        loadPage: boundSolutionPageLoader,
        commentForKey: (key) => solutionCommentByKey[key]
      })
    : null;
  $: exportableSolutionKeys =
    solutionKeysComplete && solutionCount === solutionKeys.length ? solutionKeys : [];
  $: showFinesseDetails = Boolean(
    finesseView && (
      finesseView.exactTotalInputs !== null ||
      finesseView.policyResults.some(hasFinessePolicyDetails)
    )
  );
  $: finalBoardMask = parseBoardMask(summary.build_final_board_mask);
  $: canContinue = view.status === 'completed' && report?.solution_found === true && finalBoardMask !== null;
  $: hasOutput = Boolean(view.response || report || view.publicFailures.length);
  $: label = (
    key: Parameters<typeof workspaceMessage>[1],
    values: Record<string, string | number> = {}
  ) => workspaceMessage(language, key, values);

  function number(value: number | undefined): string {
    return value === undefined ? '—' : new Intl.NumberFormat(language).format(value);
  }

  function summaryNumber(value: string | undefined): number | undefined {
    if (value === undefined) return undefined;
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : undefined;
  }

  function inputCount(value: string | number | undefined | null): string {
    return formatFinesseInputCount(value, language);
  }

  function hasFinessePolicyDetails(policy: ClearraFinessePolicyResult): boolean {
    return !policy.complete ||
      policy.successful_probability_mass != null ||
      policy.oracle_on_covered_average_inputs != null ||
      policy.information_penalty_inputs != null ||
      policy.success_probability_gap != null;
  }

  function buildSolutionComments(): Record<string, string> {
    return Object.fromEntries(
      solutionKeys.flatMap((key) => {
        const parts: string[] = [];
        const probability = solutionProbabilityByKey[key];
        if (probability) {
          parts.push(
            `${label('solutionProbability')}: ${workspaceProbability(language, probability.probability)}`
          );
        }
        for (const finesse of finesseView?.solutionByKey[key] ?? []) {
          const policy = label(
            finesse.policy === 'oracle' ? 'finesseOraclePolicy' : 'finesseVisibleSevenPolicy'
          );
          const materialized = finesse.complete ? '' : ` (${label('finesseMaterialized')})`;
          parts.push(
            `${label('finesseSolutionAverageInputs')} (${policy}): ${inputCount(finesse.average_inputs)}${materialized}`
          );
        }
        return parts.length ? [[key, parts.join(' | ')]] : [];
      })
    );
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

  function failedQueueIndex(key: string): number {
    const value = Number(key.slice('failed_pattern_'.length));
    return Number.isSafeInteger(value) ? value : Number.MAX_SAFE_INTEGER;
  }

  async function copyFailedQueues() {
    if (!failedQueueEntries.length) return;
    try {
      await writeClipboardText(failedQueueEntries.join('\n'));
      failedQueueCopyComplete = true;
      setTimeout(() => {
        failedQueueCopyComplete = false;
      }, 1600);
    } catch {
      failedQueueCopyComplete = false;
    }
  }
</script>

<ResultWorkspaceFrame
  ariaLabel={label('buildProbabilityResults')}
  status={view.status}
  statusLabel={label(view.status)}
  elapsedLabel={label('elapsed')}
  elapsedText={`${(elapsedMs / 1000).toFixed(1)}s`}
  progressProfile={effectiveAggregation === 'tiling' ? 'tiling' : 'build'}
  progressMode={effectiveAggregation === 'spin' ? 'build-spin' : 'buildability'}
  {language}
  progressLabel={(workspaceProgressLabel(language, view.progressTelemetry) ?? view.progressLabel) || label('idle')}
  progressDetail={workspaceProgressDetail(language, view.progressTelemetry)}
  progressDone={view.progressDone}
  progressTotal={view.progressTotal}
  progressTelemetry={view.progressTelemetry}
  publicFailures={view.publicFailures}
>
    {#if !hasOutput && view.status === 'idle'}
      <div class="empty-state"><Search size={28} strokeWidth={1.5} /><p>{label('noBuildProbabilityResult')}</p></div>
    {:else if view.status !== 'failed' && view.status !== 'terminated' && !productResultPayload && (aggregationAuthority.state === 'rejected' || coverageAggregation.state === 'rejected')}
      <div class="aggregation-authority-error" role="alert">
        <p>
          {label(
            aggregationAuthority.state === 'rejected'
              ? 'buildProbabilityAggregationMismatch'
              : 'buildProbabilityCoverageAggregationInvalid'
          )}
        </p>
      </div>
    {:else if view.status !== 'failed' && view.status !== 'terminated'}
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
            {#if effectiveAggregation === 'tiling'}
              <span>{label('tilingCount')}</span>
              <strong>{solutionCount === null ? label('notCalculated') : number(solutionCount)}</strong>
              <small>{label('tilingOnlyWarning')}</small>
            {:else if effectiveAggregation === 'spin'}
              <span>{label('spinSearchProbability')}</span>
              <strong>{workspaceProbability(language, summary.spin_search_probability)}</strong>
              <small>{number(summaryNumber(summary.spin_search_candidate_count))} {label('spinSearchBuilds')} · {label('spinAccuracy')}: {summary.spin_search_accuracy ?? '—'}{summary.build_mirror_included === 'true' ? ` · ${label('originalAndMirror')}` : ''}</small>
            {:else}
              <div class:with-averages={Boolean(finesseView?.policyResults.length)} class="probability-with-inputs">
                <div
                  class="primary-probability"
                  data-metric-id={BUILD_PROBABILITY_PRIMARY_METRIC.id}
                  data-future-visibility={BUILD_PROBABILITY_PRIMARY_METRIC.futureVisibility}
                  data-queue-knowledge={BUILD_PROBABILITY_PRIMARY_METRIC.queueKnowledge}
                  data-distinct-from={BUILD_PROBABILITY_PRIMARY_METRIC.distinctFrom}
                >
                  <span>{label('oracleBuildProbability')}</span>
                  <strong>{workspaceProbability(language, buildCoverSummary?.successProbability ?? authorizedCoverage?.successProbability)}</strong>
                </div>
                {#if finesseView?.policyResults.length}
                  <div class="average-input-list" aria-label={label('finesseOverallAverageInputs')}>
                    {#each finesseView.policyResults as policyResult}
                      <div class="average-input">
                        <span>
                          {label('finesseOverallAverageInputs')} ·
                          {label(policyResult.policy === 'oracle' ? 'finesseOraclePolicy' : 'finesseVisibleSevenPolicy')}
                        </span>
                        <strong>{inputCount(policyResult.overall_average_inputs)}</strong>
                        {#if !policyResult.complete}<small>{label('finesseMaterialized')}</small>{/if}
                      </div>
                    {/each}
                  </div>
                {/if}
              </div>
              <small class="metric-footnote">{number(buildCoverSummary?.successfulPatternCount ?? authorizedCoverage?.successfulPatternCount)} / {number(buildCoverSummary?.patternCount ?? authorizedCoverage?.patternCount)} {label('patterns')}{summary.build_mirror_included === 'true' ? ` · ${label('originalAndMirror')}` : ''}</small>
            {/if}
          </div>
          {#if effectiveAggregation === 'spin'}
            <div class="spin-metric">
              <div class:with-averages={Boolean(finesseView?.policyResults.length)} class="probability-with-inputs">
                <div
                  class="primary-probability"
                  data-metric-id={BUILD_PROBABILITY_PRIMARY_METRIC.id}
                  data-future-visibility={BUILD_PROBABILITY_PRIMARY_METRIC.futureVisibility}
                  data-queue-knowledge={BUILD_PROBABILITY_PRIMARY_METRIC.queueKnowledge}
                  data-distinct-from={BUILD_PROBABILITY_PRIMARY_METRIC.distinctFrom}
                >
                  <span>{label('oracleBuildProbability')}</span>
                  <strong>{workspaceProbability(language, authorizedCoverage?.successProbability)}</strong>
                </div>
                {#if finesseView?.policyResults.length}
                  <div class="average-input-list" aria-label={label('finesseOverallAverageInputs')}>
                    {#each finesseView.policyResults as policyResult}
                      <div class="average-input">
                        <span>
                          {label('finesseOverallAverageInputs')} ·
                          {label(policyResult.policy === 'oracle' ? 'finesseOraclePolicy' : 'finesseVisibleSevenPolicy')}
                        </span>
                        <strong>{inputCount(policyResult.overall_average_inputs)}</strong>
                        {#if !policyResult.complete}<small>{label('finesseMaterialized')}</small>{/if}
                      </div>
                    {/each}
                  </div>
                {/if}
              </div>
              <small class="metric-footnote">{number(authorizedCoverage?.successfulPatternCount)} / {number(authorizedCoverage?.patternCount)} {label('patterns')}</small>
            </div>
          {/if}
          {#if finesseView && showFinesseDetails}
            <div class="finesse-metrics">
              {#if finesseView.exactTotalInputs !== null}
                <div>
                  <span>{label('finesseExactTotalInputs')}</span>
                  <strong>{inputCount(finesseView.exactTotalInputs)}</strong>
                  {#if !finesseView.complete}<small>{label('finesseMaterialized')}</small>{/if}
                </div>
              {/if}
              {#each finesseView.policyResults as policyResult}
                {#if hasFinessePolicyDetails(policyResult)}
                  <div>
                    <span>{label(policyResult.policy === 'oracle' ? 'finesseOraclePolicy' : 'finesseVisibleSevenPolicy')}</span>
                    {#if policyResult.successful_probability_mass != null}
                      <small>
                        {label('finesseSuccessProbability')}:
                        {workspaceProbability(language, policyResult.successful_probability_mass)}
                        {#if policyResult.successful_unique_queue_count != null && policyResult.total_unique_queue_count != null}
                          · {number(policyResult.successful_unique_queue_count)} / {number(policyResult.total_unique_queue_count)}
                          {label('finesseSuccessfulQueues')}
                        {/if}
                      </small>
                    {/if}
                    {#if policyResult.oracle_on_covered_average_inputs != null}
                      <small>{label('finesseOracleOnCoveredAverage')}: {inputCount(policyResult.oracle_on_covered_average_inputs)}</small>
                    {/if}
                    {#if policyResult.information_penalty_inputs != null}
                      <small>{label('finesseInformationPenalty')}: {inputCount(policyResult.information_penalty_inputs)}</small>
                    {/if}
                    {#if policyResult.success_probability_gap != null}
                      <small>{label('finesseSuccessProbabilityGap')}: {workspaceProbability(language, policyResult.success_probability_gap)}</small>
                    {/if}
                    {#if !policyResult.complete}<small>{label('finesseMaterialized')}</small>{/if}
                  </div>
                {/if}
              {/each}
            </div>
          {/if}
          <dl>
            {#if buildScoreFieldSummary}
              <div><dt>{label('overallScore')}</dt><dd>{buildScoreFieldSummary.overall_score}</dd></div>
            {/if}
            <div><dt>{label(effectiveAggregation === 'tiling' ? 'tilingCount' : 'buildableTilings')}</dt><dd>{solutionCount === null ? label('notCalculated') : number(solutionCount)}</dd></div>
            {#if authorizedCoverage}
              <div><dt>{label('successfulPatterns')}</dt><dd>{number(authorizedCoverage.successfulPatternCount)} / {number(authorizedCoverage.patternCount)}</dd></div>
              <div><dt>{label('failedPatterns')}</dt><dd>{number(authorizedCoverage.failedPatternCount)}</dd></div>
              <div><dt>{label('failureProbability')}</dt><dd>{workspaceProbability(language, authorizedCoverage.failedProbability)}</dd></div>
              <div><dt>{label('probabilityComplete')}</dt><dd>{label(authorizedCoverage.complete ? 'complete' : 'incomplete')}</dd></div>
            {/if}
            {#if effectiveAggregation !== 'tiling' && summary.build_mirror_included === 'true'}
              <div><dt>{label('originalBuildProbability')}</dt><dd>{workspaceProbability(language, summary.original_coverage_probability)}</dd></div>
              <div><dt>{label('mirrorAddedPatterns')}</dt><dd>{number(summaryNumber(summary.mirror_union_added_pattern_count))}</dd></div>
            {/if}
            <div><dt>{label('candidateTilings')}</dt><dd>{number(buildCoverSummary?.sourceCandidateCount ?? report?.packing_candidate_count)}</dd></div>
          </dl>
        </div>
      </div>

      {#if failedQueueResult}
        <section class="failed-queue-section" aria-label={label('failedQueueList')}>
          <div class="failed-queue-metrics">
            <article><span>{label('failedQueueCount')}</span><strong>{summary.failed_pattern_count ?? '—'}</strong></article>
            <article><span>{label('failedQueueProbability')}</span><strong>{workspaceProbability(language, summary.failed_queue_probability)}</strong></article>
            <article><span>{label('totalQueueCount')}</span><strong>{summary.total_pattern_count ?? '—'}</strong></article>
          </div>
          <div class="solutions-heading">
            <h3>{label('failedQueueList')}</h3>
            {#if failedQueueEntries.length}
              <button class="queue-copy-button" type="button" on:click={copyFailedQueues}>
                {#if failedQueueCopyComplete}<Check size={15} />{:else}<Copy size={15} />{/if}
                <span>{label(failedQueueCopyComplete ? 'failedQueuesCopied' : 'copyFailedQueues')}</span>
              </button>
            {/if}
          </div>
          {#if failedQueueEntries.length}
            {#if summary.failed_pattern_examples_truncated === 'true'}
              <p class="failed-queue-note">{label('resultLimited', { count: failedQueueEntries.length, total: summary.failed_pattern_count ?? failedQueueEntries.length })}</p>
            {/if}
            <div class="failed-queue-grid">
              {#each failedQueueEntries.slice(0, visibleFailedQueueCount) as queue}
                <code>{queue}</code>
              {/each}
            </div>
            {#if nextFailedQueueCount > 0}
              <button class="show-more-failed" type="button" on:click={() => (visibleFailedQueueCount += 100)}>{label('showMoreFailedQueues')}</button>
            {/if}
          {:else}
            <div class="empty-state"><Search size={28} strokeWidth={1.5} /><p>{label('noFailedQueues')}</p></div>
          {/if}
        </section>
      {:else if productResultPayload && !buildScoreFieldSummary}
        <ProductResultPager
          payload={productResultPayload}
          {language}
          targetLines={height}
          loadNextPage={loadNextProductPage}
          loadMemberPage={loadProductMemberPage}
          releasePages={releaseProductPages}
        />
      {:else}
      <section class="solutions-section" aria-label={label('solutions')} data-result-mode={resultMode}>
        <div class="solutions-heading">
          <h3>{label('solutions')}</h3>
          {#if solutionCount !== null && solutionCount > 0}
            <SolutionCopyFormatControl
              bind:value={copyFormat}
              {language}
              solutionKeys={exportableSolutionKeys}
              keySource={solutionExportKeySource}
            />
          {/if}
        </div>
        {#if solutionCount === null}
          <div class="empty-state"><Search size={28} strokeWidth={1.5} /><p>{label('solutionSetNotCalculated')}</p></div>
        {:else if solutionCount > 0 && (solutionKeys.length || boundSolutionPageLoader)}
          <SolutionGallery
            {solutionKeys}
            {solutionCount}
            loadSolutionPage={boundSolutionPageLoader}
            solutionProbabilities={solutionProbabilityByKey}
            solutionAverageScores={solutionAverageScoreByKey}
            solutionFinesse={finesseView?.solutionByKey ?? {}}
            solutionComments={solutionCommentByKey}
            representativeWitness={finesseView?.representativeWitness ?? null}
            solutionSetHash={report?.normalized_solution_set_hash ?? ''}
            targetLines={height}
            {language}
            {copyFormat}
          />
        {:else if solutionCount > 0}
          <div class="empty-state"><Search size={28} strokeWidth={1.5} /><p>{label('solutionPageLoadFailed')}</p></div>
        {:else}
          <div class="empty-state"><Search size={28} strokeWidth={1.5} /><p>{label('noSolutions')}</p></div>
        {/if}
      </section>
      {/if}
    {/if}
  </ResultWorkspaceFrame>

<style>
  .empty-state { align-items: center; color: #87918d; display: flex; flex-direction: column; justify-content: center; min-height: 220px; text-align: center; }
  .empty-state p { font-size: 13px; margin: 12px 0 0; }
  .aggregation-authority-error { background: #fff1ed; border-left: 3px solid #c45635; color: #8d3026; margin-top: 16px; padding: 12px 14px; }
  .aggregation-authority-error p { font-size: 12px; margin: 0; }
  .result-grid { display: grid; gap: 28px; grid-template-columns: minmax(260px, 430px) minmax(0, 1fr); }
  h3 { color: #36423e; font-size: 12px; margin: 0 0 10px; }
  .board-frame { background: #101817; border: 1px solid #253330; border-radius: 6px; padding: 12px; }
  .board { aspect-ratio: calc(10 / var(--board-rows)); display: grid; grid-template-columns: repeat(10, 1fr); grid-template-rows: repeat(var(--board-rows), 1fr); margin: 0 auto; max-height: 520px; max-width: 100%; }
  .board span { background: #1e2927; box-shadow: inset 0 0 0 1px rgba(216, 226, 222, .2); }
  .board span.existing { background: #737d79; box-shadow: inset 2px 2px 0 rgba(255,255,255,.1), inset -2px -2px 0 rgba(20,26,24,.25); }
  .board span.target { background: #d8e2de; box-shadow: inset 2px 2px 0 rgba(255,255,255,.16), inset -2px -2px 0 rgba(41,56,51,.18); }
  .continue-button { align-items: center; background: #fff; border: 1px solid #aebbb5; border-radius: 5px; color: #27403a; cursor: pointer; display: inline-flex; font-size: 11px; font-weight: 700; gap: 7px; margin-top: 10px; min-height: 34px; padding: 7px 10px; }
  .continue-button:disabled { cursor: default; opacity: .4; }
  .metrics-panel { min-width: 0; }
  .hero-metric { border-bottom: 1px solid #dce2de; display: grid; gap: 6px; padding: 2px 0 18px; }
  .hero-metric > span { color: #68736f; font-size: 11px; font-weight: 700; text-transform: uppercase; }
  .hero-metric strong { color: #075f58; font-size: 34px; font-weight: 780; }
  .hero-metric small { color: #6b7671; font-size: 11px; }
  .spin-metric { background: #edf6f3; border-bottom: 1px solid #cfe1dc; display: grid; gap: 5px; padding: 13px 10px; }
  .spin-metric .primary-probability strong { font-size: 22px; }
  .spin-metric small { color: #64726d; font-size: 10px; }
  .probability-with-inputs { display: grid; min-width: 0; }
  .probability-with-inputs.with-averages { align-items: stretch; gap: 14px; grid-template-columns: minmax(140px, .75fr) minmax(180px, 1.25fr); }
  .primary-probability { display: grid; gap: 4px; min-width: 0; }
  .primary-probability > span, .average-input > span { color: #68736f; font-size: 10px; font-weight: 700; }
  .average-input-list { display: grid; gap: 6px; grid-template-columns: repeat(auto-fit, minmax(128px, 1fr)); min-width: 0; }
  .average-input { border-left: 1px solid #d5ded9; display: grid; gap: 3px; min-width: 0; padding-left: 12px; }
  .average-input strong { font-size: 19px; }
  .average-input small { color: #8a5b36; font-size: 10px; font-weight: 700; }
  .finesse-metrics { display: grid; gap: 1px; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); margin-top: 1px; }
  .finesse-metrics > div { background: #f0f3f1; display: grid; gap: 4px; padding: 12px 10px; }
  .finesse-metrics span { color: #68736f; font-size: 10px; font-weight: 700; }
  .finesse-metrics strong { color: #075f58; font-size: 19px; }
  .finesse-metrics small { color: #8a5b36; font-size: 10px; font-weight: 700; }
  dl { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); margin: 16px 0 0; }
  dl div { align-items: baseline; border-bottom: 1px solid #e5e9e6; display: flex; gap: 12px; justify-content: space-between; padding: 10px; }
  dt { color: #68736f; font-size: 11px; }
  dd { color: #24312d; font-size: 12px; font-weight: 700; margin: 0; overflow-wrap: anywhere; text-align: right; }
  .solutions-section { border-top: 1px solid #dce2de; margin-top: 24px; padding-top: 20px; }
  .failed-queue-section { border-top: 1px solid #dce2de; display: grid; gap: 12px; margin-top: 24px; padding-top: 20px; }
  .failed-queue-metrics { display: grid; gap: 1px; grid-template-columns: repeat(3, minmax(0, 1fr)); }
  .failed-queue-metrics article { background: #f0f3f1; display: grid; gap: 4px; padding: 12px 10px; }
  .failed-queue-metrics span, .failed-queue-note { color: #68736f; font-size: 10px; }
  .failed-queue-metrics strong { color: #075f58; font-size: 19px; }
  .failed-queue-grid { display: grid; gap: 6px; grid-template-columns: repeat(auto-fill, minmax(110px, 1fr)); }
  .failed-queue-grid code { background: #f3f5f4; border: 1px solid #d7ded9; border-radius: 5px; color: #24312d; font-size: 11px; padding: 8px; }
  .queue-copy-button, .show-more-failed { align-items: center; background: #fff; border: 1px solid #aebbb5; border-radius: 5px; color: #27403a; cursor: pointer; display: inline-flex; font-size: 11px; font-weight: 700; gap: 7px; min-height: 34px; padding: 7px 10px; }
  .show-more-failed { justify-self: center; }
  .solutions-heading { align-items: center; display: flex; gap: 16px; justify-content: space-between; margin-bottom: 12px; }
  .solutions-heading h3 { margin: 0; }
  @media (max-width: 780px) { .result-grid { grid-template-columns: 1fr; } dl { grid-template-columns: 1fr; } }
  @media (max-width: 520px) {
    .probability-with-inputs.with-averages { grid-template-columns: 1fr; }
    .average-input { border-left: 0; border-top: 1px solid #d5ded9; padding-left: 0; padding-top: 9px; }
  }
</style>
