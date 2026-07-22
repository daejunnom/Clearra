<script lang="ts">
  import { onDestroy, onMount } from 'svelte';

  import {
    cancelDesktopJob,
    desktopJobState,
    disposeDesktopJobPolling,
    resumeDesktopJobPolling,
    startDesktopJob,
    updateDesktopRequest
  } from '../stores';
  import RenderStatusPanel from '../render/RenderStatusPanel.svelte';
  import QueueTextInput from './QueueTextInput.svelte';

  $: state = $desktopJobState;
  $: capability = state.result?.capability_report.render_capability ?? null;
  $: jobActive = state.status === 'running' || state.status === 'cancelling';
  $: progressMaximum = Math.max(state.progressTotal, 1);

  onDestroy(disposeDesktopJobPolling);
  onMount(() => {
    resumeDesktopJobPolling();
  });
</script>

<main class="shell">
  <section class="workspace">
    <header class="topbar">
      <div>
        <h1>Clearra</h1>
        <p>{state.status}</p>
      </div>
      <div class="actions">
        <button type="button" on:click={startDesktopJob} disabled={jobActive || !state.request.queue}>
          Run
        </button>
        <button
          class="cancel"
          type="button"
          on:click={cancelDesktopJob}
          disabled={!jobActive || state.status === 'cancelling'}
        >
          Cancel
        </button>
      </div>
    </header>

    <div class="layout">
      <form class="panel" aria-label="PC request">
        <label>
          Lines
          <input
            type="number"
            min="1"
            max="6"
            value={state.request.lines}
            on:input={(event) =>
              updateDesktopRequest({
                lines: Number((event.currentTarget as HTMLInputElement).value)
              })}
          />
        </label>

        <label>
          Fixed queue
          <QueueTextInput
            type="text"
            value={state.request.queue}
            placeholder="IIOOO"
            on:value={(event) => updateDesktopRequest({ queue: event.detail })}
          />
        </label>

        <label>
          <input
            type="checkbox"
            checked={state.request.hold_enabled}
            on:change={(event) =>
              updateDesktopRequest({
                hold_enabled: (event.currentTarget as HTMLInputElement).checked
              })}
          />
          Hold enabled
        </label>

        <label>
          Backend
          <select
            value={state.request.backend}
            on:change={(event) =>
              updateDesktopRequest({
                backend: (event.currentTarget as HTMLSelectElement)
                  .value as typeof state.request.backend
              })}
          >
            <option value="auto">Auto</option>
            <option value="cpu">CPU</option>
            <option value="gpu">GPU</option>
            <option value="hybrid">Hybrid</option>
          </select>
        </label>

        <label>
          Rule
          <select
            value={state.request.rule}
            on:change={(event) =>
              updateDesktopRequest({ rule: (event.currentTarget as HTMLSelectElement).value })}
          >
            <option value="srs-plus">SRS+</option>
            <option value="srs">SRS</option>
            <option value="no-kick">NoKick</option>
          </select>
        </label>
      </form>

      <section class="panel" aria-label="Backend status">
        <h2>Backend</h2>
        <dl>
          <div>
            <dt>Requested</dt>
            <dd>{state.backendStatus?.backend_requested ?? state.request.backend}</dd>
          </div>
          <div>
            <dt>Selected</dt>
            <dd>{state.backendStatus?.backend_selected ?? 'pending'}</dd>
          </div>
          <div>
            <dt>Fallback</dt>
            <dd>{state.backendStatus?.fallback_used ? 'used' : 'none'}</dd>
          </div>
          <div>
            <dt>Boundary</dt>
            <dd>clearra-app/AppRequest</dd>
          </div>
          <div>
            <dt>Job</dt>
            <dd>{state.jobId ?? 'none'}</dd>
          </div>
        </dl>
      </section>

      <RenderStatusPanel {capability} />
    </div>

    <section class="job-status" aria-label="Job progress">
      <div class="progress-heading">
        <h2>{state.progressLabel || 'Job'}</h2>
        <span>{state.progressDone} / {state.progressTotal}</span>
      </div>
      <progress value={state.progressDone} max={progressMaximum}></progress>
      <dl class="runtime-status">
        <div>
          <dt>Budget</dt>
          <dd>{state.resourceStatus?.budget_status ?? 'pending'}</dd>
        </div>
        <div>
          <dt>Memory</dt>
          <dd>{state.memoryStatus?.state ?? 'pending'}</dd>
        </div>
        <div>
          <dt>Complete</dt>
          <dd>{state.resourceStatus?.probability_complete ?? 'pending'}</dd>
        </div>
      </dl>
    </section>

    <section class="diagnostics" aria-label="Diagnostics">
      <h2>Diagnostics</h2>
      {#if state.diagnostics.length === 0}
        <p>None</p>
      {:else}
        <ul>
          {#each state.diagnostics as diagnostic}
            <li><strong>{diagnostic.code}</strong><span>{diagnostic.severity}</span></li>
          {/each}
        </ul>
      {/if}
    </section>

    <section class="result" aria-label="Result">
      <pre>{JSON.stringify(state.result ?? state.validation ?? { status: state.status, error: state.error }, null, 2)}</pre>
    </section>
  </section>
</main>

<style>
  :global(body) {
    margin: 0;
    background: #101114;
    color: #f4f4f5;
    font-family:
      Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  }

  .shell {
    min-height: 100vh;
  }

  .workspace {
    margin: 0 auto;
    max-width: 1180px;
    padding: 24px;
  }

  .topbar,
  .layout {
    display: grid;
    gap: 16px;
  }

  .topbar {
    align-items: center;
    grid-template-columns: 1fr auto;
  }

  .actions {
    display: flex;
    gap: 8px;
  }

  h1,
  h2,
  p {
    margin: 0;
  }

  h1 {
    font-size: 28px;
    font-weight: 700;
  }

  h2 {
    font-size: 14px;
    font-weight: 700;
  }

  p,
  dt {
    color: #a1a1aa;
    font-size: 13px;
  }

  .layout {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .panel,
  .job-status,
  .diagnostics,
  .result {
    border: 1px solid #2b2d33;
    border-radius: 8px;
    background: #17191f;
    padding: 16px;
  }

  .job-status,
  .diagnostics {
    margin-top: 16px;
  }

  form {
    display: grid;
    gap: 14px;
  }

  label {
    display: grid;
    gap: 6px;
    color: #d4d4d8;
    font-size: 13px;
  }

  input,
  select,
  button {
    border: 1px solid #3f424b;
    border-radius: 6px;
    background: #20232b;
    color: #f4f4f5;
    font: inherit;
    min-height: 36px;
    padding: 0 10px;
  }

  button.cancel {
    background: transparent;
    color: #f4f4f5;
  }

  button {
    background: #e5e7eb;
    color: #111827;
    cursor: pointer;
    font-weight: 700;
  }

  button:disabled {
    cursor: default;
    opacity: 0.55;
  }

  dl {
    display: grid;
    gap: 12px;
    margin: 16px 0 0;
  }

  .progress-heading {
    align-items: center;
    display: flex;
    justify-content: space-between;
    margin-bottom: 10px;
  }

  .progress-heading span {
    color: #a1a1aa;
    font-size: 12px;
  }

  progress {
    accent-color: #22c55e;
    display: block;
    height: 10px;
    width: 100%;
  }

  .runtime-status {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .diagnostics ul {
    display: grid;
    gap: 8px;
    list-style: none;
    margin: 12px 0 0;
    padding: 0;
  }

  .diagnostics li {
    align-items: center;
    border-top: 1px solid #2b2d33;
    display: flex;
    font-size: 13px;
    justify-content: space-between;
    padding-top: 8px;
  }

  .diagnostics li span {
    color: #a1a1aa;
  }

  dl div {
    display: flex;
    justify-content: space-between;
    gap: 16px;
  }

  dd {
    margin: 0;
    font-size: 13px;
  }

  .result {
    margin-top: 16px;
  }

  pre {
    margin: 0;
    overflow: auto;
    white-space: pre-wrap;
  }

  @media (max-width: 840px) {
    .layout,
    .topbar,
    .runtime-status {
      grid-template-columns: 1fr;
    }
  }
</style>
