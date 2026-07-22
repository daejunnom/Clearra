<script lang="ts">
  import { AlertTriangle, CheckCircle2, ListTree, Search } from '@lucide/svelte';

  import type { WorkspaceRuntimeStatus } from './workspaceRuntime';

  export type WorkspaceResultTab = 'overview' | 'solutions' | 'diagnostics';

  export let ariaLabel: string;
  export let status: WorkspaceRuntimeStatus;
  export let statusLabel: string;
  export let elapsedLabel: string;
  export let elapsedText: string;
  export let runtimeTitle: string;
  export let runtimeLabel: string;
  export let progressAriaLabel: string;
  export let progressLabel: string;
  export let progressDetail = '';
  export let progressDone = 0;
  export let progressTotal = 0;
  export let progressDoneText: string;
  export let progressTotalText: string;
  export let overviewLabel: string;
  export let solutionsLabel: string;
  export let solutionCountText: string;
  export let diagnosticsLabel: string;
  export let diagnosticCountText: string;

  let activeTab: WorkspaceResultTab = 'overview';

  $: progressMaximum = Math.max(1, progressTotal);
  $: progressPercent = Math.min(100, (progressDone / progressMaximum) * 100);
</script>

<section class="result-workspace" aria-label={ariaLabel}>
  <div class="result-heading">
    <div>
      <span class="eyebrow">{ariaLabel}</span>
      <div class="status-line">
        {#if status === 'completed'}
          <CheckCircle2 size={18} strokeWidth={2} />
        {:else if status === 'failed'}
          <AlertTriangle size={18} strokeWidth={2} />
        {:else}
          <Search size={18} strokeWidth={1.8} />
        {/if}
        <strong>{statusLabel}</strong>
      </div>
    </div>
    <div class="heading-metrics">
      <span>{elapsedLabel} <strong>{elapsedText}</strong></span>
      <span>{runtimeTitle} <strong>{runtimeLabel}</strong></span>
    </div>
  </div>

  <div class="progress-track" aria-label={progressAriaLabel}>
    <span style={`width:${progressPercent}%`}></span>
  </div>
  <div class="progress-meta">
    <span>{progressLabel}</span>
    <span>{progressDoneText} / {progressTotalText}</span>
  </div>
  {#if progressDetail}
    <div class="progress-detail">{progressDetail}</div>
  {/if}

  <nav class="result-tabs" aria-label={ariaLabel}>
    <button
      type="button"
      class:active={activeTab === 'overview'}
      aria-pressed={activeTab === 'overview'}
      on:click={() => (activeTab = 'overview')}
    ><ListTree size={15} />{overviewLabel}</button>
    <button
      type="button"
      class:active={activeTab === 'solutions'}
      aria-pressed={activeTab === 'solutions'}
      on:click={() => (activeTab = 'solutions')}
    ><Search size={15} />{solutionsLabel}<span class="count">{solutionCountText}</span></button>
    <button
      type="button"
      class:active={activeTab === 'diagnostics'}
      aria-pressed={activeTab === 'diagnostics'}
      on:click={() => (activeTab = 'diagnostics')}
    ><AlertTriangle size={15} />{diagnosticsLabel}<span class="count">{diagnosticCountText}</span></button>
  </nav>

  <div class="result-body"><slot {activeTab} /></div>
</section>

<style>
  .result-workspace {
    background: #ffffff;
    border-top: 1px solid #d7ded9;
    padding: 24px max(24px, calc((100vw - 1460px) / 2));
  }

  .result-heading,
  .status-line,
  .heading-metrics,
  .result-tabs {
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

  .progress-track {
    background: #e5eae7;
    border-radius: 3px;
    height: 5px;
    margin-top: 18px;
    overflow: hidden;
  }

  .progress-track span {
    background: #16877d;
    display: block;
    height: 100%;
    min-width: 0;
    transition: width 160ms ease;
  }

  .progress-meta {
    color: #737e79;
    display: flex;
    font-size: 11px;
    justify-content: space-between;
    margin-top: 6px;
  }

  .progress-detail {
    color: #4f5d58;
    font-size: 11px;
    line-height: 1.45;
    margin-top: 4px;
    min-height: 16px;
    overflow-wrap: anywhere;
  }

  .result-tabs {
    border-bottom: 1px solid #dfe4e1;
    gap: 4px;
    margin-top: 22px;
    overflow-x: auto;
  }

  .result-tabs button {
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

  .result-tabs button.active {
    box-shadow: inset 0 -2px #16877d;
    color: #075f58;
  }

  .count {
    background: #e9eeeb;
    border-radius: 3px;
    font-size: 10px;
    padding: 2px 5px;
  }

  .result-body {
    min-height: 250px;
    padding: 20px 0 6px;
  }

  @media (max-width: 820px) {
    .result-heading {
      align-items: flex-start;
      flex-direction: column;
    }

    .result-body {
      overflow-x: auto;
    }
  }

  @media (max-width: 520px) {
    .result-workspace {
      padding-left: 16px;
      padding-right: 16px;
    }
  }
</style>
