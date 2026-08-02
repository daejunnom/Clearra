<script lang="ts">
  import { onDestroy } from 'svelte';

  import { updateWasmCommandText, wasmWorkerState } from './wasmWorkerStore';
  import { WasmTerminalWorkerController } from './WasmTerminalWorkerController';

  export let workerFactory: (() => Worker) | null = null;

  const workerController = new WasmTerminalWorkerController(workerFactory);
  $: state = $wasmWorkerState;
  $: workerController.setWorkerFactory(workerFactory);

  onDestroy(() => workerController.dispose());
</script>

<main class="wasm-shell">
  <section class="command-band">
    <label>
      Command
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
      >Run</button>
      <button
        data-testid="cancel-job"
        type="button"
        on:click={() => workerController.cancel()}
        disabled={state.status !== 'running' || state.jobId === null}
      >Cancel</button>
    </div>
  </section>

  <section class="status-grid">
    <div class="panel">
      <h2>Runtime</h2>
      <dl>
        <div>
          <dt>Status</dt>
          <dd data-testid="runtime-status">{state.status}</dd>
        </div>
        <div>
          <dt>Boundary</dt>
          <dd>{state.response?.capability_report.app_request_boundary ?? 'pending'}</dd>
        </div>
        <div>
          <dt>App Status</dt>
          <dd>{state.response?.status ?? 'pending'}</dd>
        </div>
      </dl>
    </div>

    <div class="panel">
      <h2>Worker</h2>
      <dl>
        <div>
          <dt>Job</dt>
          <dd>{state.jobId ?? 'none'}</dd>
        </div>
        <div>
          <dt>Progress</dt>
          <dd>{state.progressDone}/{state.progressTotal}</dd>
        </div>
        <div>
          <dt>Output</dt>
          <dd>{state.response?.result?.kind ?? 'pending'}</dd>
        </div>
        <div>
          <dt>Backend</dt>
          <dd>{state.searchReport?.backend_selected ?? 'pending'}</dd>
        </div>
        <div>
          <dt>Workers</dt>
          <dd>{state.searchReport
              ? `${state.searchReport.workers_used} (${state.searchReport.cpu_parallel_execution ? 'parallel' : 'serial'})`
              : 'pending'}</dd>
        </div>
        <div>
          <dt>Solutions</dt>
          <dd>{state.searchReport?.unique_solution_count ?? 'pending'}</dd>
        </div>
        <div>
          <dt>Solution hash</dt>
          <dd>{state.searchReport?.normalized_solution_set_hash ?? 'pending'}</dd>
        </div>
        <div>
          <dt>Coverage</dt>
          <dd>{state.searchReport
              ? `${state.searchReport.covered_pattern_count}/${state.searchReport.materialized_pattern_count}`
              : 'pending'}</dd>
        </div>
      </dl>
    </div>

    <div class="panel">
      <h2>WebGPU</h2>
      <dl>
        <div>
          <dt>Connected</dt>
          <dd>{state.webgpuBackend
              ? String(state.webgpuBackend.outcome_state === 'Connected')
              : 'pending'}</dd>
        </div>
        <div>
          <dt>Fallback</dt>
          <dd>{state.webgpuBackend
              ? state.webgpuBackend.fallback_used
                ? (state.webgpuBackend.fallback_backend ?? 'unknown')
                : 'false'
              : 'pending'}</dd>
        </div>
        <div>
          <dt>Trust</dt>
          <dd>{state.webgpuBackend?.gpu_trust_state ?? 'pending'}</dd>
        </div>
        <div>
          <dt>Reason</dt>
          <dd>{state.webgpuBackend
              ? (state.webgpuBackend.webgpu_unavailable_reason ?? 'none')
              : 'pending'}</dd>
        </div>
        <div>
          <dt>Shader</dt>
          <dd>{state.webgpuBackend?.shader.shader_hash || 'pending'}</dd>
        </div>
        <div>
          <dt>Warmup</dt>
          <dd>{state.webgpuBackend ? String(state.webgpuBackend.gpu_warmup_performed) : 'pending'}</dd>
        </div>
        <div>
          <dt>Session reused</dt>
          <dd>{state.webgpuBackend ? String(state.webgpuBackend.gpu_session_reused) : 'pending'}</dd>
        </div>
      </dl>
    </div>
  </section>

  <section class="terminal" aria-label="terminal-like output">
    <pre>{state.terminalLines.join('\n')}</pre>
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
