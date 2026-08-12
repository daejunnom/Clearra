<script lang="ts">
  import { AlertTriangle, CheckCircle2, LoaderCircle, Search } from '@lucide/svelte';

  import SolutionCopyFormatControl from './SolutionCopyFormatControl.svelte';
  import SolutionGallery from './SolutionGallery.svelte';
  import type { SolutionCopyFormat } from './solutionExport';
  import type { SolutionExportKeySource } from './solutionExportAsync';
  import {
    workspaceSolutionCount,
    workspaceSolutionPageAvailable
  } from './solutionSetAvailability';
  import type { WorkspaceRuntimeView } from './workspaceRuntime';
  import {
    workspaceMessage,
    workspaceProbability,
    type WorkspaceLanguage
  } from './workspaceI18n';

  export let view: WorkspaceRuntimeView;
  export let language: WorkspaceLanguage;
  export let elapsedMs = 0;
  export let targetLines = 4;
  export let loadSolutionPage:
    | ((offset: number, limit: number) => Promise<{ keys: string[]; total: number }>)
    | null = null;

  const EXPORT_PAGE_SIZE = 1_000;
  let copyFormat: SolutionCopyFormat = 'ctk';

  $: report = view.searchReport;
  $: solutionKeys = report?.normalized_solution_keys ?? [];
  $: summaryFields = Object.fromEntries(report?.summary_fields ?? []);
  $: solutionPageAvailable = workspaceSolutionPageAvailable(report);
  $: solutionCount = workspaceSolutionCount(report);
  $: resultIncomplete = view.status === 'completed' && (
    report?.count_complete === false || view.resourceReport?.truncated === true
  );
  $: exportKeySource = solutionCount !== null && solutionPageAvailable && loadSolutionPage
    ? createExportKeySource(solutionCount, loadSolutionPage)
    : null;
  $: progressPercent = view.progressTotal > 0
    ? Math.max(0, Math.min(100, (view.progressDone / view.progressTotal) * 100))
    : 0;
  $: failureMessages = Array.from(new Set([
    ...(view.error ? [view.error] : []),
    ...view.diagnostics.map((diagnostic) => diagnostic.message).filter(Boolean)
  ]));
  $: label = (
    key: Parameters<typeof workspaceMessage>[1],
    values: Record<string, string | number> = {}
  ) => workspaceMessage(language, key, values);

  function number(value: number): string {
    return new Intl.NumberFormat(language).format(value);
  }

  function createExportKeySource(
    keyCount: number,
    loader: NonNullable<typeof loadSolutionPage>
  ): SolutionExportKeySource | null {
    if (keyCount < 1) return null;
    return {
      keyCount,
      async readKeys(start, count, signal) {
        if (
          !Number.isSafeInteger(start) ||
          !Number.isSafeInteger(count) ||
          start < 0 ||
          count < 0 ||
          start + count > keyCount
        ) {
          throw new RangeError('PC solver export range is invalid');
        }
        const keys: string[] = [];
        while (keys.length < count) {
          throwIfAborted(signal);
          const response = await loader(
            start + keys.length,
            Math.min(EXPORT_PAGE_SIZE, count - keys.length)
          );
          if (!response.keys.length) {
            throw new Error('PC solver solution store ended before the reported total');
          }
          keys.push(...response.keys.slice(0, count - keys.length));
        }
        throwIfAborted(signal);
        return keys;
      }
    };
  }

  function throwIfAborted(signal: AbortSignal | undefined) {
    if (!signal?.aborted) return;
    if (signal.reason instanceof Error) throw signal.reason;
    const error = new Error('Solution copy was aborted.');
    error.name = 'AbortError';
    throw error;
  }
</script>

{#if view.status !== 'idle'}
  <section class="solver-result" aria-label={label('results')}>
    <header>
      <div class="status">
        {#if view.status === 'completed'}
          <CheckCircle2 size={18} strokeWidth={2} />
        {:else if view.status === 'failed' || view.status === 'terminated'}
          <AlertTriangle size={18} strokeWidth={2} />
        {:else if view.status === 'cancelled'}
          <Search size={18} strokeWidth={1.8} />
        {:else}
          <span class="spinner"><LoaderCircle size={18} strokeWidth={1.8} /></span>
        {/if}
        <strong>{label(view.status)}</strong>
      </div>
      <span>{(elapsedMs / 1000).toFixed(1)}s</span>
    </header>

    {#if view.status === 'running' || view.status === 'validating' || view.status === 'cancelling'}
      <div
        class="progress"
        class:indeterminate={view.progressTotal <= 0}
        role="progressbar"
        aria-valuemin="0"
        aria-valuemax={view.progressTotal || 1}
        aria-valuenow={view.progressTotal > 0 ? view.progressDone : undefined}
      ><i style={`width:${progressPercent}%`}></i></div>
    {/if}

    {#if view.status === 'failed' || view.status === 'terminated'}
      <div class="failure" role="alert">
        {#each failureMessages as message}<p>{message}</p>{/each}
      </div>
    {:else if view.status === 'completed'}
      {#if resultIncomplete}
        <p class="incomplete" role="status">{label('playerFinderResultsIncomplete')}</p>
      {/if}
      <div class="result-summary">
        <div><strong>{solutionCount === null ? label('notCalculated') : number(solutionCount)}</strong><span>{label('solutions')}</span></div>
        <div><strong>{workspaceProbability(language, report?.coverage_probability)}</strong><span>{label('coverage')}</span></div>
        {#if solutionCount !== null}
          <SolutionCopyFormatControl
            bind:value={copyFormat}
            {language}
            compact
            {solutionKeys}
            keySource={exportKeySource}
          />
        {/if}
      </div>

      {#if solutionCount === null}
        <div class="empty"><Search size={24} strokeWidth={1.5} /><span>{label('solutionSetNotCalculated')}</span></div>
      {:else if solutionCount > 0}
        <SolutionGallery
          {solutionKeys}
          {solutionCount}
          loadSolutionPage={solutionPageAvailable ? loadSolutionPage : null}
          solutionSetHash={report?.normalized_solution_set_hash ?? ''}
          {targetLines}
          {language}
          {copyFormat}
        />
      {:else}
        <div class="empty"><Search size={24} strokeWidth={1.5} /><span>{label(resultIncomplete ? 'playerFinderNoConclusion' : 'noSolutions')}</span></div>
      {/if}
    {/if}
  </section>
{/if}

<style>
  .solver-result { border-top: 1px solid #d9dfdb; margin: 0 auto; max-width: 1180px; padding: 22px 0 40px; }
  header, .status, .result-summary, .result-summary > div { align-items: center; display: flex; }
  header { color: #68736f; font-size: 12px; justify-content: space-between; }
  .status { color: #08766d; gap: 8px; }
  .status strong { color: #17211e; font-size: 16px; }
  .spinner { animation: spin 900ms linear infinite; display: inline-flex; }
  .progress { background: #e5ebe7; height: 3px; margin-top: 14px; overflow: hidden; position: relative; }
  .progress i { background: #16877d; display: block; height: 100%; transition: width 120ms linear; }
  .progress.indeterminate i { animation: sweep 1.1s ease-in-out infinite; left: -30%; position: absolute; width: 30% !important; }
  .failure { background: #fff1ed; border-left: 3px solid #c45635; color: #8d3026; margin-top: 16px; padding: 10px 13px; }
  .failure p { font-size: 12px; margin: 0; overflow-wrap: anywhere; }
  .failure p + p { margin-top: 4px; }
  .incomplete { background: #fff7df; border-left: 3px solid #c89b2f; color: #654a0e; font-size: 11px; line-height: 1.5; margin: 16px 0 0; padding: 9px 11px; }
  .result-summary { border-bottom: 1px solid #e0e5e2; gap: 28px; margin: 18px 0; padding-bottom: 16px; }
  .result-summary > div { gap: 7px; }
  .result-summary strong { color: #17211e; font-size: 21px; }
  .result-summary span { color: #68736f; font-size: 11px; }
  .result-summary > :global(:last-child) { margin-left: auto; }
  .empty { align-items: center; color: #87918d; display: flex; gap: 9px; justify-content: center; min-height: 120px; }
  .empty span { font-size: 12px; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @keyframes sweep { from { transform: translateX(0); } to { transform: translateX(430%); } }
  @media (max-width: 1228px) { .solver-result { margin-left: 24px; margin-right: 24px; } }
  @media (max-width: 620px) {
    .solver-result { margin-left: 16px; margin-right: 16px; padding-bottom: 28px; }
    .result-summary { align-items: flex-start; flex-wrap: wrap; gap: 10px 20px; }
    .result-summary > :global(:last-child) { flex: 1 1 100%; margin-left: 0; }
  }
  @media (prefers-reduced-motion: reduce) {
    .spinner, .progress.indeterminate i { animation: none; }
  }
</style>
