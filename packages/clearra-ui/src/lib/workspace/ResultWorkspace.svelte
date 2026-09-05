<script lang="ts">
  import { Check, ChevronDown, Copy, Search } from '@lucide/svelte';

  import { writeClipboardText } from './clipboardText';
  import ResultWorkspaceFrame from './ResultWorkspaceFrame.svelte';
  import ProductResultPager from './ProductResultPager.svelte';
  import {
    productResultOwnsSolutionPage,
    type ProductMemberPageLoader,
    type ProductNextPageLoader,
    type ProductPageRelease
  } from './productResultPager';
  import SolutionCopyFormatControl from './SolutionCopyFormatControl.svelte';
  import SolutionGallery from './SolutionGallery.svelte';
  import type { ScoreMode } from './solverWorkspaceModel';
  import type { SolutionCopyFormat } from './solutionExport';
  import type { SolutionExportKeySource } from './solutionExportAsync';
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
  export let targetLines = 4;
  export let tilingOnlyRequested = false;
  export let failedQueueRequested = false;
  export let scoreMode: ScoreMode = 'off';
  export let loadSolutionPage: SolutionPageLoader | null = null;
  export let loadNextProductPage: ProductNextPageLoader | null = null;
  export let loadProductMemberPage: ProductMemberPageLoader | null = null;
  export let releaseProductPages: ProductPageRelease | null = null;

  let copyFormat: SolutionCopyFormat = 'ctk';
  let failedQueueCopyComplete = false;
  let visibleFailedQueueCount = 100;
  let failedQueueResultIdentity = '';

  $: report = view.searchReport;
  $: productResultPayload = view.response?.product_result_payload ?? null;
  $: productSolutionPageActive = productResultOwnsSolutionPage(productResultPayload);
  $: pcScoreFieldSummary = productResultPayload?.content.payload_kind === 'pc-score-field-summary'
    ? productResultPayload.content.payload
    : null;
  $: solutionCount = pcScoreFieldSummary
    ? pcScoreFieldSummary.fields.length
    : workspaceSolutionCount(report);
  $: canonicalSolutionKeys = pcScoreFieldSummary
    ? pcScoreFieldSummary.fields.map((field) => field.normalized_field_key)
    : (report?.normalized_solution_keys ?? []);
  $: solutionProbabilityByKey = Object.fromEntries(
    (report?.solution_probabilities ?? []).map((entry) => [entry.solution_key, entry])
  );
  $: solutionAverageScoreByKey = pcScoreFieldSummary
    ? Object.fromEntries(
        pcScoreFieldSummary.fields.map((field) => [field.normalized_field_key, field])
      )
    : Object.fromEntries(
        (report?.solution_average_scores ?? []).map((entry) => [entry.solution_key, entry])
      );
  $: solutionCommentByKey = buildSolutionComments(
    canonicalSolutionKeys,
    language,
    solutionProbabilityByKey,
    solutionAverageScoreByKey
  );
  $: solutionCommentsAvailable =
    (report?.solution_probabilities.length ?? 0) > 0 ||
    (pcScoreFieldSummary?.fields.length ?? report?.solution_average_scores.length ?? 0) > 0;
  $: solutionKeys = canonicalSolutionKeys
    .map((key, canonicalIndex) => ({
      canonicalIndex,
      key,
      probability: solutionProbabilityValue(key)
    }))
    .sort(compareSolutionProbability)
    .map((entry) => entry.key);
  $: summaryFields = Object.fromEntries(report?.summary_fields ?? []);
  $: failedQueueResult = report
    ? summaryFields.result_mode === 'failed-queue'
    : failedQueueRequested;
  $: failedQueueEntries = (report?.summary_fields ?? [])
    .filter(([key]) => /^failed_pattern_\d+$/.test(key))
    .sort(([left], [right]) => failedQueueIndex(left) - failedQueueIndex(right))
    .map(([, queue]) => queue);
  $: currentFailedQueueResultIdentity = failedQueueResult
    ? `${summaryFields.total_pattern_count ?? ''}:${summaryFields.failed_pattern_count ?? ''}:${failedQueueEntries[0] ?? ''}`
    : '';
  $: if (currentFailedQueueResultIdentity !== failedQueueResultIdentity) {
    failedQueueResultIdentity = currentFailedQueueResultIdentity;
    visibleFailedQueueCount = 100;
    failedQueueCopyComplete = false;
  }
  $: tilingOnly = summaryFields.objective === 'tiling';
  $: solutionPageAvailable = pcScoreFieldSummary
    ? false
    : workspaceSolutionPageAvailable(report);
  $: solutionKeysComplete = pcScoreFieldSummary
    ? true
    : workspaceSolutionKeysComplete(report);
  $: solutionResultIdentity = solutionPageResultIdentity(
    report?.normalized_solution_set_hash,
    solutionCount,
    canonicalSolutionKeys
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
  $: tilingProgress = tilingOnly || (!report && tilingOnlyRequested);
  $: scoringRequested = summaryFields.score_requested === 'true';
  $: progressMode = failedQueueResult
    ? 'pc-failed-queue' as const
    : summaryFields.objective === 'minimum-cover' ||
        scoreMode === 'minimum-cover' ||
        scoreMode === 'score-minimals'
      ? 'pc-minimum-cover' as const
      : scoringRequested || scoreMode === 'summary' || scoreMode === 'score-finder'
        ? 'pc-score' as const
        : 'pc-all' as const;
  $: solutionExportKeySource = createSolutionExportKeySource();
  $: exportableSolutionKeys =
    solutionKeysComplete && solutionCount === solutionKeys.length ? solutionKeys : [];
  $: hasResult = Boolean(view.response || report);
  $: label = (
    key: Parameters<typeof workspaceMessage>[1],
    values: Record<string, string | number> = {}
  ) => workspaceMessage(language, key, values);

  function number(value: number | undefined): string {
    return value === undefined ? '—' : new Intl.NumberFormat(language).format(value);
  }

  function solutionProbabilityValue(key: string): number | null {
    const raw = solutionProbabilityByKey[key]?.probability;
    if (raw === undefined) return null;
    const value = Number(raw);
    return Number.isFinite(value) ? value : null;
  }

  function compareSolutionProbability(
    left: { canonicalIndex: number; probability: number | null },
    right: { canonicalIndex: number; probability: number | null }
  ): number {
    if (left.probability === null) {
      return right.probability === null ? left.canonicalIndex - right.canonicalIndex : 1;
    }
    if (right.probability === null) return -1;
    return right.probability - left.probability || left.canonicalIndex - right.canonicalIndex;
  }

  function exactBuildVariantCount(): string {
    if (report?.build_variant_count_exact !== 'true') return '—';
    return number(report.build_variant_count);
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

  function createSolutionExportKeySource(): SolutionExportKeySource | null {
    if (solutionCount === null) return null;
    const loader = boundSolutionPageLoader;
    if (loader) {
      return createPagedSolutionExportKeySource({
        keyCount: solutionCount,
        loadPage: loader,
        ...(solutionCommentsAvailable
          ? { commentForKey: (key: string) => solutionCommentByKey[key] }
          : {})
      });
    }
    if (
      !solutionKeysComplete ||
      solutionCount !== solutionKeys.length ||
      !solutionCommentsAvailable
    ) return null;
    const localKeys = solutionKeys;
    const keyCount = localKeys.length;
    if (keyCount < 1) return null;
    return {
      keyCount,
      ...(solutionCommentsAvailable
        ? { commentForKey: (key: string) => solutionCommentByKey[key] }
        : {}),
      async readKeys(start, count, signal) {
        if (
          !Number.isSafeInteger(start) ||
          !Number.isSafeInteger(count) ||
          start < 0 ||
          count < 0 ||
          count > keyCount ||
          start > keyCount - count
        ) {
          throw new RangeError('tiling solution export range is invalid');
        }
        throwIfAborted(signal);
        return localKeys.slice(start, start + count);
      }
    };
  }

  function buildSolutionComments(
    keys: string[],
    selectedLanguage: WorkspaceLanguage,
    probabilities: typeof solutionProbabilityByKey,
    scores: typeof solutionAverageScoreByKey
  ): Record<string, string> {
    return Object.fromEntries(
      keys.flatMap((key) => {
        const parts: string[] = [];
        const probability = probabilities[key];
        if (probability) {
          parts.push(
            `${workspaceMessage(selectedLanguage, 'solutionProbability')}: ${probabilityLabel(probability.probability)}`
          );
        }
        const score = scores[key];
        if (score) {
          parts.push(
            `${workspaceMessage(selectedLanguage, 'solutionAverageScore')}: ${scoreLabel(score.average_score)}`
          );
        }
        return parts.length ? [[key, parts.join(' | ')]] : [];
      })
    );
  }

  function throwIfAborted(signal: AbortSignal | undefined) {
    if (!signal?.aborted) return;
    if (signal.reason instanceof Error) throw signal.reason;
    const error = new Error('Solution copy was aborted.');
    error.name = 'AbortError';
    throw error;
  }

  function failedQueueIndex(key: string): number {
    const value = Number(key.slice('failed_pattern_'.length));
    return Number.isFinite(value) ? value : Number.MAX_SAFE_INTEGER;
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
  ariaLabel={label('results')}
  status={view.status}
  statusLabel={label(view.status)}
  elapsedLabel={label('elapsed')}
  elapsedText={`${(elapsedMs / 1000).toFixed(1)}s`}
  progressProfile={tilingProgress ? 'tiling' : 'pc'}
  {progressMode}
  {language}
  progressLabel={(workspaceProgressLabel(language, view.progressTelemetry) ?? view.progressLabel) || label('idle')}
  progressDetail={workspaceProgressDetail(language, view.progressTelemetry)}
  progressDone={view.progressDone}
  progressTotal={view.progressTotal}
  progressTelemetry={view.progressTelemetry}
  publicFailures={view.publicFailures}
>
  {#if !hasResult && view.status === 'idle'}
    <div class="empty-state"><Search size={28} strokeWidth={1.5} /><p>{label('noResult')}</p></div>
  {:else if view.status !== 'failed' && view.status !== 'terminated'}
    {#if failedQueueResult}
      <div class="metric-grid failed-queue-metrics">
        <article><span>{label('failedQueueCount')}</span><strong>{summaryFields.failed_pattern_count ?? '—'}</strong></article>
        <article><span>{label('failedQueueProbability')}</span><strong>{workspaceProbability(language, summaryFields.failed_queue_probability)}</strong></article>
        <article><span>{label('totalQueueCount')}</span><strong>{summaryFields.total_pattern_count ?? '—'}</strong></article>
      </div>

      <section class="solutions-section failed-queue-section" aria-label={label('failedQueueList')}>
        <div class="solutions-heading">
          <h2>{label('failedQueueList')}</h2>
          {#if failedQueueEntries.length}
            <button class="queue-copy-button" type="button" on:click={copyFailedQueues}>
              {#if failedQueueCopyComplete}<Check size={15} />{:else}<Copy size={15} />{/if}
              <span>{label(failedQueueCopyComplete ? 'failedQueuesCopied' : 'copyFailedQueues')}</span>
            </button>
          {/if}
        </div>
        {#if failedQueueEntries.length}
          <div class="failed-queue-grid">
            {#each failedQueueEntries.slice(0, visibleFailedQueueCount) as queue}
              <code>{queue}</code>
            {/each}
          </div>
          {#if visibleFailedQueueCount < failedQueueEntries.length}
            <button
              class="show-more-queues"
              type="button"
              on:click={() => (visibleFailedQueueCount += 100)}
            >
              <ChevronDown size={16} />
              <span>{label('showMoreFailedQueues')}</span>
            </button>
          {/if}
        {:else if view.status === 'completed'}
          <div class="empty-state compact"><Check size={26} strokeWidth={1.7} /><p>{label('noFailedQueues')}</p></div>
        {/if}
      </section>
    {:else}
      <div class="metric-grid" class:tiling-only={tilingOnly}>
        <article><span>{label(tilingOnly ? 'tilingCount' : 'solutionCount')}</span><strong>{solutionCount === null ? label('notCalculated') : number(solutionCount)}</strong></article>
        {#if !tilingOnly}
          <article><span>{label('coverage')}</span><strong>{workspaceProbability(language, report?.coverage_probability)}</strong></article>
          <article><span>{label('buildVariants')}</span><strong>{exactBuildVariantCount()}</strong></article>
        {/if}
      </div>

      {#if scoringRequested || pcScoreFieldSummary}
        <div class="metric-grid score-metrics">
          <article><span>{pcScoreFieldSummary ? label('overallScore') : label('averageScore')}</span><strong>{pcScoreFieldSummary?.overall_score ?? summaryFields.score_field_average_score ?? summaryFields.score_unconditional_expected_score ?? '—'}</strong></article>
        </div>
      {/if}

      <ProductResultPager
        payload={productResultPayload}
        {language}
        {targetLines}
        loadNextPage={loadNextProductPage}
        loadMemberPage={loadProductMemberPage}
        releasePages={releaseProductPages}
      />

      {#if !productSolutionPageActive}
        <section class="solutions-section" aria-label={label('solutions')}>
          <div class="solutions-heading">
            <h2>{label('solutions')}</h2>
            {#if solutionCount !== null}
              <SolutionCopyFormatControl
                bind:value={copyFormat}
                {language}
                solutionKeys={exportableSolutionKeys}
                keySource={solutionExportKeySource}
              />
            {/if}
          </div>
          {#if solutionCount === null}
            <div class="empty-state compact"><Search size={26} strokeWidth={1.5} /><p>{label('solutionSetNotCalculated')}</p></div>
          {:else if solutionCount > 0 && (solutionKeys.length || boundSolutionPageLoader)}
            <SolutionGallery
              {solutionKeys}
              {solutionCount}
              loadSolutionPage={boundSolutionPageLoader}
              solutionProbabilities={solutionProbabilityByKey}
              solutionAverageScores={solutionAverageScoreByKey}
              solutionComments={solutionCommentByKey}
              solutionSetHash={report?.normalized_solution_set_hash ?? ''}
              {targetLines}
              {language}
              {copyFormat}
            />
          {:else if solutionCount > 0}
            <div class="empty-state compact"><Search size={26} strokeWidth={1.5} /><p>{label('solutionPageLoadFailed')}</p></div>
          {:else}
            <div class="empty-state compact"><Search size={26} strokeWidth={1.5} /><p>{label('noSolutions')}</p></div>
          {/if}
        </section>
      {/if}
    {/if}
  {/if}
</ResultWorkspaceFrame>

<style>
  .metric-grid {
    display: grid;
    gap: 1px;
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .metric-grid.tiling-only {
    grid-template-columns: minmax(0, 1fr);
  }

  .metric-grid.tiling-only article {
    border-radius: 6px;
  }

  .metric-grid article {
    background: #f0f3f1;
    min-width: 0;
    padding: 14px;
  }

  .metric-grid article:first-child {
    border-radius: 6px 0 0 6px;
  }

  .metric-grid article:last-child {
    border-radius: 0 6px 6px 0;
  }

  .metric-grid span {
    color: #68736f;
    display: block;
    font-size: 11px;
  }

  .metric-grid strong {
    color: #17211e;
    display: block;
    font-size: 20px;
    margin-top: 5px;
    overflow-wrap: anywhere;
  }

  .score-metrics {
    grid-template-columns: minmax(0, 1fr);
    margin-top: 1px;
  }

  .failed-queue-grid {
    display: grid;
    gap: 6px;
    grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
  }

  .failed-queue-grid code {
    background: #f0f3f1;
    border: 1px solid #dfe5e2;
    border-radius: 4px;
    color: #22302b;
    font-size: 12px;
    overflow-wrap: anywhere;
    padding: 8px 9px;
  }

  .queue-copy-button,
  .show-more-queues {
    align-items: center;
    background: #fff;
    border: 1px solid #cbd4d0;
    border-radius: 5px;
    color: #2d3b36;
    cursor: pointer;
    display: inline-flex;
    font: inherit;
    font-size: 12px;
    font-weight: 650;
    gap: 7px;
    min-height: 34px;
    padding: 7px 10px;
  }

  .show-more-queues {
    margin-top: 14px;
  }

  .score-metrics article:first-child,
  .score-metrics article:last-child {
    border-radius: 6px;
  }

  .solutions-section {
    border-top: 1px solid #dfe4e1;
    margin-top: 24px;
    padding-top: 20px;
  }

  .solutions-heading {
    align-items: center;
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    justify-content: space-between;
    margin-bottom: 14px;
  }

  .solutions-heading h2 {
    color: #26322e;
    font-size: 14px;
    margin: 0;
  }

  .empty-state {
    align-items: center;
    color: #87918d;
    display: flex;
    flex-direction: column;
    justify-content: center;
    min-height: 220px;
    text-align: center;
  }

  .empty-state.compact {
    min-height: 150px;
  }

  .empty-state p {
    font-size: 13px;
    margin: 12px 0 0;
  }

  @media (max-width: 820px) {
    .metric-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }

    .metric-grid article:first-child,
    .metric-grid article:last-child {
      border-radius: 0;
    }
  }

  @media (max-width: 520px) {
    .metric-grid {
      grid-template-columns: 1fr;
    }
  }

  @media (pointer: coarse) {
    .queue-copy-button,
    .show-more-queues {
      min-height: 44px;
    }
  }
</style>
