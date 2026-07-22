<script lang="ts">
  import { CheckCircle2, Copy, Search } from '@lucide/svelte';

  import ResultWorkspaceFrame from './ResultWorkspaceFrame.svelte';
  import SolutionGallery from './SolutionGallery.svelte';
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

  let copied = '';

  $: report = view.searchReport;
  $: canonicalSolutionKeys = report?.normalized_solution_keys ?? [];
  $: solutionProbabilityByKey = Object.fromEntries(
    (report?.solution_probabilities ?? []).map((entry) => [entry.solution_key, entry])
  );
  $: solutionKeys = canonicalSolutionKeys
    .map((key, canonicalIndex) => ({
      canonicalIndex,
      key,
      probability: solutionProbabilityValue(key)
    }))
    .sort(compareSolutionProbability)
    .map((entry) => entry.key);
  $: backend = view.backendReport;
  $: summaryFields = Object.fromEntries(report?.summary_fields ?? []);
  $: scoringRequested = summaryFields.score_requested === 'true';
  $: scoreMatrixComplete = summaryFields.score_matrix_complete === 'true';
  $: scoreIncompleteReason =
    [
      summaryFields.objective_incomplete_reason,
      summaryFields.score_summary_incomplete_reason,
      summaryFields.score_matrix_incomplete_reason
    ].find((value) => value && value !== 'none') ?? 'none';
  $: resource = view.resourceReport;
  $: hasOutput = Boolean(view.response || report || view.diagnostics.length || view.error);
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

  function bytes(value: number | undefined): string {
    if (value === undefined) return '—';
    return `${(value / (1024 * 1024)).toFixed(value >= 100 * 1024 * 1024 ? 0 : 1)} MiB`;
  }

  function valueFrom(source: unknown, key: string): unknown {
    if (!source || typeof source !== 'object') return undefined;
    return (source as Record<string, unknown>)[key];
  }

  function textValue(source: unknown, key: string, fallback = '—'): string {
    const value = valueFrom(source, key);
    return value === undefined || value === null || value === '' ? fallback : String(value);
  }

  async function copyText(value: string, identity: string) {
    if (!value) return;
    await navigator.clipboard.writeText(value);
    copied = identity;
    window.setTimeout(() => {
      if (copied === identity) copied = '';
    }, 1400);
  }

</script>

<ResultWorkspaceFrame
  ariaLabel={label('results')}
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
      <div class="empty-state"><Search size={28} strokeWidth={1.5} /><p>{label('noResult')}</p></div>
    {:else if activeTab === 'overview'}
      <div class="metric-grid">
        <article><span>{label('solutionCount')}</span><strong>{number(report?.unique_solution_count)}</strong></article>
        <article><span>{label('coverage')}</span><strong>{workspaceProbability(language, report?.coverage_probability)}</strong></article>
        <article><span>{label('buildVariants')}</span><strong>{exactBuildVariantCount()}</strong></article>
        <article><span>{label('searchedNodes')}</span><strong>{number(report?.searched_nodes)}</strong></article>
      </div>

      {#if scoringRequested}
        <div class="metric-grid score-metrics">
          <article><span>{label('averageScore')}</span><strong>{summaryFields.score_field_average_score ?? summaryFields.score_unconditional_expected_score ?? '—'}</strong></article>
          <article><span>{label('scoreAccuracy')}</span><strong>{summaryFields.score_accuracy_level ?? '—'}</strong></article>
          <article><span>{label('scoreStatus')}</span><strong>{label(scoreMatrixComplete ? 'complete' : 'incomplete')}</strong></article>
          <article><span>{label('scoreReason')}</span><strong>{scoreIncompleteReason}</strong></article>
        </div>
      {/if}

      <div class="overview-columns">
        <section>
          <h3>{label('backend')}</h3>
          <dl>
            <div><dt>{label('requestedBackend')}</dt><dd>{textValue(backend, 'backend_requested')}</dd></div>
            <div><dt>{label('actualBackend')}</dt><dd>{report?.backend_selected ?? textValue(backend, 'backend_selected')}</dd></div>
            <div><dt>{label('workersUsed')}</dt><dd>{number(report?.workers_used)}</dd></div>
            {#if scoringRequested}
              <div><dt>{label('executionDistribution')}</dt><dd>{summaryFields.score_execution_distribution ?? '—'}</dd></div>
            {/if}
            <div><dt>{label('fallbackUsed')}</dt><dd>{textValue(backend, 'fallback_used', label('no'))}</dd></div>
            <div><dt>{label('device')}</dt><dd>{textValue(backend, 'gpu_device_selected_name', view.webgpuReport?.webgpu_adapter_label_or_redacted || '—')}</dd></div>
            <div><dt>{label('trust')}</dt><dd>{view.webgpuReport?.gpu_trust_state ?? '—'}</dd></div>
            <div><dt>{label('shader')}</dt><dd class="mono">{view.webgpuReport?.shader.shader_hash ?? '—'}</dd></div>
          </dl>
        </section>
        <section>
          <h3>{label('limits')}</h3>
          <dl>
            <div><dt>{label('memory')}</dt><dd>{report ? bytes(report.peak_cpu_bytes) : textValue(resource, 'peak_cpu_bytes')}</dd></div>
            <div><dt>{label('frontier')}</dt><dd>{report ? number(report.peak_frontier_states) : textValue(resource, 'peak_frontier_states')}</dd></div>
            <div><dt>{label('probabilityComplete')}</dt><dd>{String(report?.probability_complete ?? valueFrom(resource, 'probability_complete') ?? false)}</dd></div>
            <div><dt>{label('countComplete')}</dt><dd>{String(report?.count_complete ?? false)}</dd></div>
            <div><dt>{label('truncated')}</dt><dd>{textValue(resource, 'truncated', label('no'))}</dd></div>
          </dl>
        </section>
        <section>
          <h3>{label('render')}</h3>
          <dl>
            <div><dt>PNG</dt><dd>{view.renderCapability ? label(view.renderCapability.png_supported ? 'supported' : 'unsupported') : label('pending')}</dd></div>
            <div><dt>GIF</dt><dd>{view.renderCapability ? label(view.renderCapability.gif_supported ? 'supported' : 'unsupported') : label('pending')}</dd></div>
            <div><dt>{label('exact')}</dt><dd>{view.renderCapability ? String(view.renderCapability.render_exact) : label('pending')}</dd></div>
            <div><dt>{label('reason')}</dt><dd>{view.renderCapability?.unsupported_reason ?? label('none')}</dd></div>
          </dl>
        </section>
      </div>

      {#if report?.normalized_solution_set_hash}
        <div class="hash-row">
          <div><span>{label('resultHash')}</span><code>{report.normalized_solution_set_hash}</code></div>
          <button type="button" title={label('copyHash')} aria-label={label('copyHash')} on:click={() => copyText(report?.normalized_solution_set_hash ?? '', 'hash')}><Copy size={16} />{copied === 'hash' ? label('copied') : label('copyHash')}</button>
        </div>
      {/if}

    {:else if activeTab === 'solutions'}
      {#if solutionKeys.length}
        <SolutionGallery
          {solutionKeys}
          solutionProbabilities={solutionProbabilityByKey}
          solutionSetHash={report?.normalized_solution_set_hash ?? ''}
          {targetLines}
          {language}
        />
      {:else}
        <div class="empty-state"><Search size={26} strokeWidth={1.5} /><p>{label('noSolutions')}</p></div>
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
        <div class="empty-state"><CheckCircle2 size={26} strokeWidth={1.5} /><p>{label('noDiagnostics')}</p></div>
      {/if}
    {/if}
  </ResultWorkspaceFrame>

<style>
  .hash-row,
  .hash-row > div {
    align-items: center;
    display: flex;
  }

  .hash-row button {
    align-items: center;
    background: transparent;
    border: 0;
    color: #65706c;
    cursor: pointer;
    display: inline-flex;
    font: inherit;
    font-size: 12px;
    font-weight: 700;
    gap: 6px;
    min-height: 38px;
    padding: 0 11px;
    white-space: nowrap;
  }

  .metric-grid {
    display: grid;
    gap: 1px;
    grid-template-columns: repeat(4, minmax(0, 1fr));
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

  .metric-grid span,
  .hash-row span {
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

  .overview-columns {
    display: grid;
    gap: 28px;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    margin-top: 26px;
  }

  h3 {
    color: #26322e;
    font-size: 12px;
    margin: 0 0 12px;
  }

  dl {
    display: grid;
    gap: 9px;
    margin: 0;
  }

  dl div {
    display: flex;
    gap: 12px;
    justify-content: space-between;
    min-width: 0;
  }

  dt,
  dd {
    font-size: 11px;
  }

  dt {
    color: #77817d;
  }

  dd {
    color: #26322e;
    margin: 0;
    max-width: 65%;
    overflow-wrap: anywhere;
    text-align: right;
  }

  .mono,
  code {
    font-family: ui-monospace, SFMono-Regular, Consolas, monospace;
  }

  .hash-row {
    background: #eef5f2;
    border: 1px solid #cee1da;
    border-radius: 6px;
    gap: 16px;
    justify-content: space-between;
    margin-top: 24px;
    padding: 12px 14px;
  }

  .hash-row > div {
    gap: 12px;
    min-width: 0;
  }

  .hash-row code {
    color: #174a45;
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .hash-row button {
    background: #ffffff;
    border: 1px solid #cbd3ce;
    border-radius: 5px;
    color: #34403c;
    flex: 0 0 auto;
    min-height: 32px;
  }

  .diagnostic-list {
    display: grid;
    gap: 1px;
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .diagnostic-list li {
    align-items: start;
    background: #f4f6f5;
    display: grid;
    gap: 14px;
    grid-template-columns: 72px minmax(0, 1fr);
    padding: 12px;
  }

  .diagnostic-list li > span {
    color: #8b5c19;
    font-size: 10px;
    font-weight: 800;
    text-transform: uppercase;
  }

  .diagnostic-list li.error > span {
    color: #a63d32;
  }

  .diagnostic-list strong,
  .diagnostic-list p {
    font-size: 12px;
    margin: 0;
  }

  .diagnostic-list p {
    color: #68736f;
    margin-top: 4px;
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

    .overview-columns {
      grid-template-columns: 1fr;
    }

  }

  @media (max-width: 520px) {
    .metric-grid {
      grid-template-columns: 1fr;
    }

    .hash-row,
    .hash-row > div {
      align-items: stretch;
      flex-direction: column;
    }
  }
</style>
