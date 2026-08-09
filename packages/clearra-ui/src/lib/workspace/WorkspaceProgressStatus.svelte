<script lang="ts">
  import { Check, Circle, LoaderCircle, OctagonX } from '@lucide/svelte';

  import type { ClearraSearchProgressTelemetry } from '../wasm/wasmCommandClient';
  import {
    buildWorkspaceProgressModel,
    type WorkspaceProgressMode,
    type WorkspaceProgressProfile,
    type WorkspaceProgressMetric,
    type WorkspaceProgressStage
  } from './workspaceProgressModel';
  import {
    workspaceMessage,
    type WorkspaceLanguage
  } from './workspaceI18n';
  import type { WorkspaceRuntimeStatus } from './workspaceRuntime';

  export let profile: WorkspaceProgressProfile;
  export let mode: WorkspaceProgressMode = 'default';
  export let status: WorkspaceRuntimeStatus;
  export let language: WorkspaceLanguage;
  export let progressLabel = '';
  export let progressDetail = '';
  export let progressDone = 0;
  export let progressTotal = 0;
  export let forwardPatternDone = 0;
  export let forwardPatternTotal = 0;
  export let telemetry: ClearraSearchProgressTelemetry | null = null;
  export let showWorkerMetrics = true;

  $: model = buildWorkspaceProgressModel({
    profile,
    mode,
    status,
    progressLabel,
    progressDone,
    progressTotal,
    forwardPatternDone,
    forwardPatternTotal,
    telemetry
  });
  $: activeStages = model.stages.filter((stage) => stage.status === 'running');
  $: stageSummary = workspaceMessage(language, 'progressStageSummary', {
    done: model.completedStages,
    total: model.totalStages
  });
  $: currentSummary = activeStages.length
    ? activeStages.map((stage) => workspaceMessage(language, stage.labelKey)).join(' + ')
    : workspaceMessage(
        language,
        status === 'completed'
          ? 'progressComplete'
          : status === 'failed' || status === 'cancelled' || status === 'terminated'
            ? 'progressStopped'
            : 'progressPending'
      );

  const formatters: Record<WorkspaceLanguage, Intl.NumberFormat> = {
    en: new Intl.NumberFormat('en', { notation: 'compact', maximumFractionDigits: 1 }),
    ko: new Intl.NumberFormat('ko', { notation: 'compact', maximumFractionDigits: 1 })
  };

  function count(value: string | null): string {
    if (value === null) return '';
    try {
      return new Intl.NumberFormat(language).format(BigInt(value));
    } catch {
      const parsed = Number(value);
      return Number.isFinite(parsed) ? formatters[language].format(parsed) : value;
    }
  }

  function metric(stage: WorkspaceProgressStage): string {
    if (stage.status === 'complete') return workspaceMessage(language, 'progressComplete');
    if (stage.status === 'pending') return workspaceMessage(language, 'progressPending');
    if (stage.status === 'stopped') return workspaceMessage(language, 'progressStopped');
    if (stage.done === null) return workspaceMessage(language, 'progressActive');
    if (stage.total === null) {
      return workspaceMessage(language, 'progressCountOnly', { count: count(stage.done) });
    }
    return `${count(stage.done)} / ${count(stage.total)}`;
  }

  function secondaryMetric(value: WorkspaceProgressMetric): string {
    return value.total === null
      ? count(value.value)
      : `${count(value.value)} / ${count(value.total)}`;
  }

  function visibleMetrics(stage: WorkspaceProgressStage): WorkspaceProgressMetric[] {
    return showWorkerMetrics
      ? stage.metrics
      : stage.metrics.filter((metric) => metric.labelKey !== 'progressMetricWorkers');
  }
</script>

<section class="progress-status" aria-live="polite" aria-label={workspaceMessage(language, 'progress')}>
  <div class="overall">
    <div>
      <span>{workspaceMessage(language, 'progressCurrentStage')}</span>
      <strong>{currentSummary}</strong>
    </div>
    <span class="stage-summary">{stageSummary}</span>
  </div>
  <div
    class="overall-track"
    role="progressbar"
    aria-valuemin="0"
    aria-valuemax="100"
    aria-valuenow={Math.round(model.overallPercent)}
    aria-valuetext={stageSummary}
  ><span style={`width:${model.overallPercent}%`}></span></div>

  <ol class="stage-list" style={`--stage-count:${model.totalStages}`}>
    {#each model.stages as stage}
      <li class:running={stage.status === 'running'} class:complete={stage.status === 'complete'} class:stopped={stage.status === 'stopped'}>
        <div class="stage-heading">
          {#if stage.status === 'complete'}
            <Check size={14} strokeWidth={2.2} />
          {:else if stage.status === 'running'}
            <LoaderCircle class="spin" size={14} strokeWidth={1.9} />
          {:else if stage.status === 'stopped'}
            <OctagonX size={14} strokeWidth={1.9} />
          {:else}
            <Circle size={12} strokeWidth={1.5} />
          {/if}
          <strong>{workspaceMessage(language, stage.labelKey)}</strong>
          <span>{metric(stage)}</span>
        </div>
        <div class="stage-metrics" aria-hidden={visibleMetrics(stage).length === 0}>
          {#each visibleMetrics(stage) as stageMetric}
            <span>
              {workspaceMessage(language, stageMetric.labelKey)}
              <b>{secondaryMetric(stageMetric)}</b>
            </span>
          {/each}
        </div>
        {#if stage.status === 'running' || (stage.status === 'stopped' && stage.percent !== null)}
          <div
            class:indeterminate={stage.status === 'running' && stage.percent === null}
            class="stage-track"
          >
            <span style={stage.percent === null ? '' : `width:${stage.percent}%`}></span>
          </div>
        {/if}
      </li>
    {/each}
  </ol>

  {#if progressDetail}
    <p class="telemetry-detail">{progressDetail}</p>
  {/if}
</section>

<style>
  .progress-status {
    border-bottom: 1px solid #dfe4e1;
    border-top: 1px solid #dfe4e1;
    margin-top: 18px;
    padding: 12px 0;
  }

  .overall,
  .stage-heading {
    align-items: center;
    display: flex;
  }

  .overall {
    gap: 16px;
    justify-content: space-between;
  }

  .overall > div {
    align-items: baseline;
    display: flex;
    gap: 8px;
    min-width: 0;
  }

  .overall span,
  .stage-summary {
    color: #697570;
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
  }

  .overall strong {
    color: #174d47;
    font-size: 12px;
    overflow-wrap: anywhere;
  }

  .overall-track,
  .stage-track {
    background: #e5eae7;
    overflow: hidden;
  }

  .overall-track {
    height: 5px;
    margin-top: 9px;
  }

  .overall-track > span,
  .stage-track > span {
    background: #16877d;
    display: block;
    height: 100%;
    transition: width 120ms linear;
  }

  .stage-list {
    display: grid;
    grid-template-columns: repeat(var(--stage-count), minmax(0, 1fr));
    list-style: none;
    margin: 12px 0 0;
    padding: 0;
  }

  .stage-list li {
    border-left: 1px solid #e2e7e4;
    min-width: 0;
    padding: 2px 12px;
  }

  .stage-list li:first-child {
    border-left: 0;
    padding-left: 0;
  }

  .stage-list li:last-child {
    padding-right: 0;
  }

  .stage-heading {
    color: #85908b;
    gap: 5px;
    min-height: 18px;
  }

  .stage-heading strong {
    color: #53605b;
    font-size: 10px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .stage-heading span {
    color: #7b8681;
    font-size: 9px;
    font-variant-numeric: tabular-nums;
    margin-left: auto;
    white-space: nowrap;
  }

  li.running .stage-heading,
  li.running .stage-heading strong,
  li.complete .stage-heading {
    color: #08756c;
  }

  li.stopped .stage-heading,
  li.stopped .stage-heading strong {
    color: #9a5449;
  }

  .telemetry-detail {
    color: #697570;
    font-size: 9px;
    line-height: 1.35;
  }

  .stage-track {
    height: 3px;
    margin-top: 6px;
    position: relative;
  }

  .stage-metrics {
    display: flex;
    flex-wrap: nowrap;
    gap: 3px 8px;
    margin-top: 5px;
    min-height: 11px;
    overflow: hidden;
  }

  .stage-metrics span {
    color: #79847f;
    font-size: 8px;
    white-space: nowrap;
  }

  .stage-metrics b {
    color: #40514b;
    font-variant-numeric: tabular-nums;
    font-weight: 750;
    margin-left: 3px;
  }

  .stage-track.indeterminate > span {
    animation: stage-progress 1.1s ease-in-out infinite;
    left: 0;
    position: absolute;
    width: 36%;
  }

  .telemetry-detail {
    font-variant-numeric: tabular-nums;
    margin: 9px 0 0;
    min-height: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .progress-status :global(.spin) {
    animation: progress-spin 900ms linear infinite;
  }

  @keyframes progress-spin {
    to { transform: rotate(360deg); }
  }

  @keyframes stage-progress {
    0% { transform: translateX(-110%); }
    100% { transform: translateX(310%); }
  }

  @media (max-width: 900px) {
    .stage-list {
      grid-template-columns: repeat(2, minmax(0, 1fr));
      row-gap: 12px;
    }

    .stage-list li:nth-child(odd) {
      border-left: 0;
      padding-left: 0;
    }
  }

  @media (max-width: 520px) {
    .overall {
      align-items: flex-start;
      flex-direction: column;
      gap: 5px;
    }

    .stage-list {
      grid-template-columns: 1fr;
    }

    .stage-list li,
    .stage-list li:nth-child(odd) {
      border-left: 0;
      border-top: 1px solid #e2e7e4;
      padding: 9px 0 0;
    }

    .stage-list li:first-child {
      border-top: 0;
      padding-top: 0;
    }

  }

  @media (prefers-reduced-motion: reduce) {
    .progress-status :global(.spin),
    .stage-track.indeterminate > span { animation: none; }
    .stage-track.indeterminate > span { left: 32%; }
  }
</style>
