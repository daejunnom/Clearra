<script lang="ts">
  import { AlertTriangle, CheckCircle2, Search } from '@lucide/svelte';

  import type { ClearraSearchProgressTelemetry } from '../wasm/wasmCommandClient';
  import WorkspaceFailureNotice from './WorkspaceFailureNotice.svelte';
  import WorkspaceProgressStatus from './WorkspaceProgressStatus.svelte';
  import type { WorkspaceLanguage } from './workspaceI18n';
  import type {
    WorkspaceProgressMode,
    WorkspaceProgressProfile
  } from './workspaceProgressModel';
  import type { WorkspaceRuntimeStatus } from './workspaceRuntime';
  import type { WorkspacePublicFailure } from './workspacePublicFailure';

  export let ariaLabel: string;
  export let status: WorkspaceRuntimeStatus;
  export let statusLabel: string;
  export let elapsedLabel: string;
  export let elapsedText: string;
  export let progressProfile: WorkspaceProgressProfile;
  export let progressMode: WorkspaceProgressMode = 'default';
  export let language: WorkspaceLanguage;
  export let progressLabel: string;
  export let progressDetail = '';
  export let progressDone = 0;
  export let progressTotal = 0;
  export let progressTelemetry: ClearraSearchProgressTelemetry | null = null;
  export let showWorkerMetrics = true;
  export let forwardPatternDone = 0;
  export let forwardPatternTotal = 0;
  export let publicFailures: WorkspacePublicFailure[] = [];

  let displayedTelemetry: ClearraSearchProgressTelemetry | null = null;
  let rememberedStatus = status;

  $: if (status !== rememberedStatus) {
    if (status === 'idle' || status === 'validating') displayedTelemetry = null;
    rememberedStatus = status;
  }
  $: if (
    progressTelemetry !== null &&
    status !== 'idle' &&
    status !== 'validating'
  ) {
    displayedTelemetry = progressTelemetry;
  }
</script>

<section class="result-workspace" aria-label={ariaLabel}>
  <div class="result-pinned">
    <div class="result-heading">
      <div>
        <span class="eyebrow">{ariaLabel}</span>
        <div class="status-line">
          {#if status === 'completed'}
            <CheckCircle2 size={18} strokeWidth={2} />
          {:else if status === 'failed' || status === 'terminated'}
            <AlertTriangle size={18} strokeWidth={2} />
          {:else}
            <Search size={18} strokeWidth={1.8} />
          {/if}
          <strong>{statusLabel}</strong>
        </div>
      </div>
      <div class="heading-metrics">
        <span>{elapsedLabel} <strong>{elapsedText}</strong></span>
      </div>
    </div>

    <WorkspaceProgressStatus
      profile={progressProfile}
      mode={progressMode}
      {status}
      {language}
      {progressLabel}
      {progressDetail}
      {progressDone}
      {progressTotal}
      {forwardPatternDone}
      {forwardPatternTotal}
      telemetry={displayedTelemetry}
      {showWorkerMetrics}
    />

    {#if status === 'failed' || status === 'terminated'}
      <WorkspaceFailureNotice failures={publicFailures} {language} heading={statusLabel} />
    {/if}
  </div>

  <div class="result-body"><slot /></div>
</section>

<style>
  .result-workspace {
    background: #ffffff;
    border-top: 1px solid #d7ded9;
    padding: 24px max(24px, calc((100vw - 1460px) / 2));
  }

  .result-pinned {
    background: #fff;
    position: sticky;
    top: 0;
    z-index: 30;
  }

  .result-heading,
  .status-line,
  .heading-metrics {
    align-items: center;
    display: flex;
  }

  .result-heading {
    gap: 20px;
    justify-content: space-between;
  }

  .eyebrow {
    color: #68736f;
    display: block;
    font-size: 11px;
    font-weight: 750;
    margin-bottom: 4px;
    text-transform: uppercase;
  }

  .status-line {
    color: #075f58;
    gap: 8px;
  }

  .status-line strong {
    color: #17211e;
    font-size: 18px;
  }

  .heading-metrics {
    color: #68736f;
    flex-wrap: wrap;
    font-size: 12px;
    gap: 20px;
  }

  .heading-metrics strong {
    color: #26322e;
    margin-left: 5px;
  }

  .result-body {
    min-height: 250px;
    padding: 20px 0 6px;
  }

  @media (max-width: 820px) {
    .result-pinned {
      position: static;
    }

    .result-heading {
      align-items: flex-start;
      flex-direction: column;
    }

  }

  @media (max-width: 520px) {
    .result-workspace {
      padding-left: 16px;
      padding-right: 16px;
    }
  }
</style>
