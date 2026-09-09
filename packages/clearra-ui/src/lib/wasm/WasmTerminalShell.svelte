<script lang="ts">
  import { runtimeShellCopy, runtimeShellValue, runtimeShellText, formatRuntimeShellTranscript } from '../i18n/runtimeShellCatalog';
  import { matchReleasedWorkspaceLanguage, type WorkspaceLanguage } from '../i18n/languageManifest';

  import { onDestroy } from 'svelte';

  import { updateWasmCommandText, wasmWorkerState } from './wasmWorkerStore';
  import { WasmTerminalWorkerController } from './WasmTerminalWorkerController';
  import { workspaceSolutionCount } from '../workspace/solutionSetAvailability';

  export let workerFactory: (() => Worker) | null = null;
  export let language: WorkspaceLanguage = 'en';

  $: locale = matchReleasedWorkspaceLanguage(language) ?? 'en';
  $: copy = runtimeShellCopy(locale);

  const workerController = new WasmTerminalWorkerController(workerFactory);
  $: state = $wasmWorkerState;
  $: solutionCount = workspaceSolutionCount(state.searchReport);
  $: workerController.setWorkerFactory(workerFactory);

  onDestroy(() => workerController.dispose());
</script>

<main class="wasm-shell">
  <section class="command-band">
    <label>
      {copy.command}
      <input
        value={state.request.commandText}
        on:input={(event) =>
          updateWasmCommandText((event.currentTarget as HTMLInputElement).value)}
      />
    </label>
    <div class="actions">
      <button
        data-testid="run-job"
        type="button"
        on:click={() => workerController.run()}
        disabled={state.status === 'running' || state.status === 'cancelling'}
      >{copy.run}</button>
      <button
        data-testid="cancel-job"
        type="button"
        on:click={() => workerController.cancel()}
        disabled={state.status !== 'running' || state.jobId === null}
      >{copy.cancel}</button>
    </div>
  </section>

  <section class="status-grid">
    <div class="panel">
      <h2>{copy.runtime}</h2>
      <dl>
        <div>
          <dt>{copy.status}</dt>
          <dd data-testid="runtime-status">{runtimeShellValue(locale, state.status)}</dd>
        </div>
        <div>
          <dt>{copy.boundary}</dt>
          <dd>{state.response?.capability_report.app_request_boundary ?? copy.pending}</dd>
        </div>
        <div>
          <dt>{copy.appStatus}</dt>
          <dd>{runtimeShellValue(locale, state.response?.status ?? copy.pending)}</dd>
        </div>
      </dl>
    </div>

    <div class="panel">
      <h2>{copy.worker}</h2>
      <dl>
        <div>
          <dt>{copy.job}</dt>
          <dd>{state.jobId ?? copy.none}</dd>
        </div>
        <div>
          <dt>{copy.progress}</dt>
          <dd>{state.progressDone}/{state.progressTotal}</dd>
        </div>
        <div>
          <dt>{copy.output}</dt>
          <dd>{state.response?.result?.kind ?? copy.pending}</dd>
        </div>
        <div>
          <dt>{copy.backend}</dt>
          <dd>{runtimeShellValue(locale, state.searchReport?.backend_selected ?? copy.pending)}</dd>
        </div>
        <div>
          <dt>{copy.workers}</dt>
          <dd>{state.searchReport
              ? runtimeShellText(locale, 'workersValue', { count: state.searchReport.workers_used, mode: state.searchReport.cpu_parallel_execution ? copy.parallel : copy.serial })
              : copy.pending}</dd>
        </div>
        <div>
          <dt>{copy.solutions}</dt>
          <dd>{state.searchReport ? (solutionCount ?? copy.notCalculated) : copy.pending}</dd>
        </div>
        <div>
          <dt>{copy.solutionHash}</dt>
          <dd>{state.searchReport?.normalized_solution_set_hash ?? copy.pending}</dd>
        </div>
        <div>
          <dt>{copy.coverage}</dt>
          <dd>{state.searchReport
              ? `${state.searchReport.covered_pattern_count}/${state.searchReport.materialized_pattern_count}`
              : copy.pending}</dd>
        </div>
      </dl>
    </div>

    <div class="panel">
      <h2>WebGPU</h2>
      <dl>
        <div>
          <dt>{copy.connected}</dt>
          <dd>{state.webgpuBackend
              ? runtimeShellValue(locale, state.webgpuBackend.outcome_state === 'Connected')
              : copy.pending}</dd>
        </div>
        <div>
          <dt>{copy.fallback}</dt>
          <dd>{state.webgpuBackend
              ? state.webgpuBackend.fallback_used
                ? runtimeShellValue(locale, state.webgpuBackend.fallback_backend ?? 'unknown')
                : copy.false
              : copy.pending}</dd>
        </div>
        <div>
          <dt>{copy.trust}</dt>
          <dd>{runtimeShellValue(locale, state.webgpuBackend?.gpu_trust_state ?? copy.pending)}</dd>
        </div>
        <div>
          <dt>{copy.reason}</dt>
          <dd>{state.webgpuBackend
              ? runtimeShellValue(locale, state.webgpuBackend.webgpu_unavailable_reason ?? 'none')
              : copy.pending}</dd>
        </div>
        <div>
          <dt>{copy.shader}</dt>
          <dd>{state.webgpuBackend?.shader.shader_hash || copy.pending}</dd>
        </div>
        <div>
          <dt>{copy.warmup}</dt>
          <dd>{state.webgpuBackend ? runtimeShellValue(locale, state.webgpuBackend.gpu_warmup_performed) : copy.pending}</dd>
        </div>
        <div>
          <dt>{copy.sessionReused}</dt>
          <dd>{state.webgpuBackend ? runtimeShellValue(locale, state.webgpuBackend.gpu_session_reused) : copy.pending}</dd>
        </div>
      </dl>
    </div>
  </section>

  <section class="terminal" aria-label={copy.terminalOutput}>
    <pre>{formatRuntimeShellTranscript(locale, state.terminalLines)}</pre>
  </section>
</main>

<style>
  :global(body) {
    margin: 0;
    background: #0f1115;
    color: #f5f5f5;
    font-family:
      Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
  }

  .wasm-shell {
    display: grid;
    gap: 16px;
    margin: 0 auto;
    max-width: 1180px;
    min-height: 100vh;
    padding: 24px;
  }

  .command-band,
  .status-grid {
    display: grid;
    gap: 16px;
  }

  .command-band {
    align-items: end;
    grid-template-columns: 1fr auto;
  }

  .status-grid {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  label,
  .actions {
    display: grid;
    gap: 8px;
  }

  .actions {
    grid-template-columns: repeat(2, 92px);
  }

  input,
  button {
    border: 1px solid #3f4654;
    border-radius: 6px;
    background: #1a1f29;
    color: #f5f5f5;
    font: inherit;
    min-height: 38px;
    padding: 0 12px;
  }

  button {
    background: #e8eef8;
    color: #101114;
    cursor: pointer;
    font-weight: 700;
  }

  button:disabled {
    cursor: default;
    opacity: 0.5;
  }

  .panel,
  .terminal {
    border: 1px solid #2a3040;
    border-radius: 8px;
    background: #151922;
    padding: 16px;
  }

  h2,
  dl,
  pre {
    margin: 0;
  }

  h2 {
    font-size: 14px;
  }

  dl {
    display: grid;
    gap: 10px;
    margin-top: 14px;
  }

  dl div {
    display: flex;
    justify-content: space-between;
    gap: 16px;
  }

  dt {
    color: #aab1c1;
  }

  dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
    text-align: right;
  }

  .terminal {
    min-height: 280px;
  }

  pre {
    overflow: auto;
    white-space: pre-wrap;
  }

  @media (max-width: 760px) {
    .command-band,
    .status-grid {
      grid-template-columns: 1fr;
    }

    .actions {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
