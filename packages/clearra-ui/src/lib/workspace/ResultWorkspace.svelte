<script lang="ts">
  import { Search } from '@lucide/svelte';

  import ResultWorkspaceFrame from './ResultWorkspaceFrame.svelte';
  import SolutionCopyFormatControl from './SolutionCopyFormatControl.svelte';
  import SolutionGallery from './SolutionGallery.svelte';
  import type { SolutionCopyFormat } from './solutionExport';
  import type { SolutionExportKeySource } from './solutionExportAsync';
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
  export let loadSolutionPage:
    | ((offset: number, limit: number) => Promise<{ keys: string[]; total: number }>)
    | null = null;

  let copyFormat: SolutionCopyFormat = 'ctk';
  const SOLUTION_EXPORT_PAGE_SIZE = 1_000;

  $: report = view.searchReport;
  $: canonicalSolutionKeys = report?.normalized_solution_keys ?? [];
  $: solutionProbabilityByKey = Object.fromEntries(
    (report?.solution_probabilities ?? []).map((entry) => [entry.solution_key, entry])
  );
  $: solutionAverageScoreByKey = Object.fromEntries(
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
    (report?.solution_average_scores.length ?? 0) > 0;
  $: solutionKeys = canonicalSolutionKeys
    .map((key, canonicalIndex) => ({
      canonicalIndex,
      key,
      probability: solutionProbabilityValue(key)
    }))
    .sort(compareSolutionProbability)
    .map((entry) => entry.key);
  $: summaryFields = Object.fromEntries(report?.summary_fields ?? []);
  $: tilingOnly = summaryFields.objective === 'tiling';
  $: solutionPageAvailable = summaryFields.solution_page_available === 'true';
  $: tilingProgress = tilingOnly || (!report && tilingOnlyRequested);
  $: scoringRequested = summaryFields.score_requested === 'true';
  $: solutionExportKeySource =
    (solutionPageAvailable && loadSolutionPage) || solutionCommentsAvailable
      ? createSolutionExportKeySource()
      : null;
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
    const loader = loadSolutionPage;
    const lazy = solutionPageAvailable && Boolean(loader);
    const localKeys = solutionKeys;
    const keyCount = lazy ? (report?.unique_solution_count ?? 0) : localKeys.length;
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
          start + count > keyCount
        ) {
          throw new RangeError('tiling solution export range is invalid');
        }
        if (!lazy || !loader) {
          throwIfAborted(signal);
          return localKeys.slice(start, start + count);
        }
        const keys: string[] = [];
        while (keys.length < count) {
          throwIfAborted(signal);
          const offset = start + keys.length;
          const response = await loader(
            offset,
            Math.min(SOLUTION_EXPORT_PAGE_SIZE, count - keys.length)
          );
          if (!response.keys.length) {
            throw new Error(
              'tiling solution page store ended before the reported total'
            );
          }
          keys.push(...response.keys.slice(0, count - keys.length));
        }
        throwIfAborted(signal);
        return keys;
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

</script>

<ResultWorkspaceFrame
  ariaLabel={label('results')}
  status={view.status}
  statusLabel={label(view.status)}
  elapsedLabel={label('elapsed')}
  elapsedText={`${(elapsedMs / 1000).toFixed(1)}s`}
  progressProfile={tilingProgress ? 'tiling' : 'pc'}
  {language}
  progressLabel={(workspaceProgressLabel(language, view.progressTelemetry) ?? view.progressLabel) || label('idle')}
  progressDetail={workspaceProgressDetail(language, view.progressTelemetry)}
  progressDone={view.progressDone}
  progressTotal={view.progressTotal}
  progressTelemetry={view.progressTelemetry}
  failureDiagnostics={view.diagnostics}
  failureMessage={view.error ?? ''}
>
  {#if !hasResult && view.status === 'idle'}
    <div class="empty-state"><Search size={28} strokeWidth={1.5} /><p>{label('noResult')}</p></div>
  {:else if view.status !== 'failed' && view.status !== 'terminated'}
    <div class="metric-grid" class:tiling-only={tilingOnly}>
      <article><span>{label(tilingOnly ? 'tilingCount' : 'solutionCount')}</span><strong>{number(report?.unique_solution_count)}</strong></article>
      {#if !tilingOnly}
        <article><span>{label('coverage')}</span><strong>{workspaceProbability(language, report?.coverage_probability)}</strong></article>
        <article><span>{label('buildVariants')}</span><strong>{exactBuildVariantCount()}</strong></article>
      {/if}
    </div>

    {#if scoringRequested}
      <div class="metric-grid score-metrics">
        <article><span>{label('averageScore')}</span><strong>{summaryFields.score_field_average_score ?? summaryFields.score_unconditional_expected_score ?? '—'}</strong></article>
      </div>
    {/if}

    <section class="solutions-section" aria-label={label('solutions')}>
      <div class="solutions-heading">
        <h2>{label('solutions')}</h2>
        <SolutionCopyFormatControl
          bind:value={copyFormat}
          {language}
          {solutionKeys}
          keySource={solutionExportKeySource}
        />
      </div>
      {#if solutionKeys.length}
        <SolutionGallery
          {solutionKeys}
          solutionCount={report?.unique_solution_count ?? solutionKeys.length}
          loadSolutionPage={solutionPageAvailable ? loadSolutionPage : null}
          solutionProbabilities={solutionProbabilityByKey}
          solutionAverageScores={solutionAverageScoreByKey}
          solutionComments={solutionCommentByKey}
          solutionSetHash={report?.normalized_solution_set_hash ?? ''}
          {targetLines}
          {language}
          {copyFormat}
        />
      {:else}
        <div class="empty-state compact"><Search size={26} strokeWidth={1.5} /><p>{label('noSolutions')}</p></div>
      {/if}
    </section>
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
</style>
