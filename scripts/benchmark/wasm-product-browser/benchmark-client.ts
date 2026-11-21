const params = new URLSearchParams(location.search);
const commandText = params.get('command');
const status = document.querySelector<HTMLPreElement>('#status');
const prewarmWorkerCount = parseOptionalPositiveInteger(params.get('prewarmWorkers'));
const prewarmGpu = params.get('prewarmGpu') === 'true';
const runtimePrewarmWorkerCount = parseOptionalPositiveInteger(
  params.get('runtimePrewarmWorkers')
);
const repeat = boundedInteger(params.get('repeat'), 1, 1, 10);
const idleSampleMs = boundedInteger(params.get('idleSampleMs'), 0, 0, 60_000);

if (
  (!commandText && prewarmWorkerCount === null) ||
  (runtimePrewarmWorkerCount !== null && !commandText) ||
  (prewarmWorkerCount !== null && (repeat !== 1 || idleSampleMs !== 0)) ||
  !status
) {
  throw new Error('benchmark command or prewarm worker count is required');
}

const started = performance.now();
const worker =
  prewarmWorkerCount === null
    ? new Worker(
        new URL('../../../apps/clearra-web/src/workers/clearraWorker.ts', import.meta.url),
        { type: 'module' }
      )
    : new Worker(new URL('./prewarm-benchmark-worker.ts', import.meta.url), {
        type: 'module'
      });
let completed = false;
let settling = false;
const samples: Record<string, unknown>[] = [];
let lastProgressPost = 0;
let runtimePrewarmStarted = started;
let runtimePrewarmElapsedMs: number | null = null;
let runStarted = started;

worker.onmessage = (message: MessageEvent<Record<string, unknown>>) => {
  if (completed || settling) return;
  const event = message.data;
  if (prewarmWorkerCount !== null) {
    if (!['completed', 'failed'].includes(String(event.type))) return;
    void complete({
      surface: 'browser-wasm-prewarm',
      elapsed_ms: performance.now() - started,
      capabilities: browserCapabilities(),
      result: event
    });
    return;
  }
  if (event.type === 'runtime_prewarm' && runtimePrewarmWorkerCount !== null) {
    if (event.phase === 'started') {
      runtimePrewarmStarted = performance.now();
    } else if (event.phase === 'finished') {
      runtimePrewarmElapsedMs = performance.now() - runtimePrewarmStarted;
      runStarted = performance.now();
      worker.postMessage({
        type: 'run_command_text',
        commandText,
        prewarmWorkerCount: runtimePrewarmWorkerCount
      });
    }
    return;
  }
  if (event.event === 'progress') {
    status.textContent = JSON.stringify(
      {
        state: 'running',
        elapsed_ms: performance.now() - started,
        progress: event.progress ?? null
      },
      null,
      2
    );
    void postProgress(event);
    return;
  }
  if (!['final_response', 'failed', 'cancelled'].includes(String(event.event))) return;
  const elapsedMs = performance.now() - started;
  void complete({
    surface: 'browser-wasm-product-worker',
    elapsed_ms: elapsedMs,
    run_elapsed_ms: performance.now() - runStarted,
    runtime_prewarm_elapsed_ms: runtimePrewarmElapsedMs,
    capabilities: browserCapabilities(),
    command: commandText,
    event: compactTerminalEvent(event)
  }, event.event === 'final_response');
};

worker.onerror = (event) => {
  const elapsedMs = performance.now() - started;
  const error = event.error instanceof Error ? event.error : null;
  void complete({
    surface: 'browser-wasm-product-worker',
    elapsed_ms: elapsedMs,
    run_elapsed_ms: performance.now() - runStarted,
    runtime_prewarm_elapsed_ms: runtimePrewarmElapsedMs,
    capabilities: browserCapabilities(),
    command: commandText,
    worker_error: event.message || error?.message || 'worker failed without a browser diagnostic',
    worker_error_name: error?.name ?? null,
    worker_error_filename: event.filename || null,
    worker_error_line: event.lineno || null,
    worker_error_column: event.colno || null
  });
};

if (prewarmWorkerCount === null) {
  if (runtimePrewarmWorkerCount === null) {
    worker.postMessage({ type: 'run_command_text', commandText });
  } else {
    worker.postMessage({
      type: 'prewarm_runtime',
      workerCount: runtimePrewarmWorkerCount
    });
  }
} else {
  worker.postMessage({ workerCount: prewarmWorkerCount, gpuWarmup: prewarmGpu });
}

async function complete(result: Record<string, unknown>, canRepeat = false) {
  if (completed || settling) return;
  settling = true;
  status!.textContent = JSON.stringify(result, null, 2);
  const terminalBrowserMemoryBytes = await boundedMemorySample();
  result.browser_memory_terminal_bytes = terminalBrowserMemoryBytes;
  let memorySampleCount = terminalBrowserMemoryBytes === null ? 0 : 1;
  if (idleSampleMs > 0) {
    // Keep the product Worker alive. A harness-level terminate here would
    // conceal any solver/realm storage still owned after the result.
    await new Promise<void>((resolve) => setTimeout(resolve, idleSampleMs));
    const idleMemory = await boundedMemorySample();
    result.browser_memory_idle_bytes = idleMemory;
    result.browser_memory_idle_delay_ms = idleSampleMs;
    if (idleMemory !== null) memorySampleCount += 1;
  }
  result.browser_memory_sample_count = memorySampleCount;
  samples.push(repeatSample(result));
  if (canRepeat && samples.length < repeat) {
    // Run again on the same product Worker, without a new browser page or
    // a synthetic prewarm. Memory sampling is outside every timed run.
    settling = false;
    lastProgressPost = 0;
    runStarted = performance.now();
    worker.postMessage({
      type: 'run_command_text',
      commandText,
      ...(runtimePrewarmWorkerCount === null ? {} : { prewarmWorkerCount: runtimePrewarmWorkerCount })
    });
    return;
  }
  completed = true;
  worker.terminate();
  const receipt = repeat === 1 ? result : { ...result, repetitions: samples };
  status!.textContent = JSON.stringify(receipt, null, 2);
  const response = await fetch('/__result', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(receipt)
  });
  if (!response.ok) throw new Error(`benchmark collector rejected result: ${response.status}`);
  document.title = 'Clearra WASM benchmark complete';
}

async function boundedMemorySample() {
  let timeout: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([
      measureBrowserMemory(),
      new Promise<null>((resolve) => {
        timeout = setTimeout(() => resolve(null), 2_000);
      })
    ]);
  } finally {
    clearTimeout(timeout);
  }
}

function repeatSample(result: Record<string, unknown>) {
  const event = result.event as Record<string, unknown> | undefined;
  const report = event?.search_report as Record<string, unknown> | undefined;
  return {
    terminal_event: event?.event ?? null,
    run_elapsed_ms: result.run_elapsed_ms,
    browser_memory_terminal_bytes: result.browser_memory_terminal_bytes,
    browser_memory_idle_bytes: result.browser_memory_idle_bytes,
    browser_memory_idle_delay_ms: result.browser_memory_idle_delay_ms,
    worker_error: result.worker_error,
    // Only one full terminal payload is emitted; repeat samples keep scalar
    // work/count/identity fields so they do not retain every large result.
    search_summary: report
      ? Object.fromEntries(Object.entries(report).filter(([, value]) =>
          value === null || typeof value === 'number' || typeof value === 'boolean' ||
          (typeof value === 'string' && value.length <= 256)))
      : null
  };
}

async function measureBrowserMemory(): Promise<number | null> {
  const measure = (
    performance as Performance & {
      measureUserAgentSpecificMemory?: () => Promise<{ bytes: number }>;
    }
  ).measureUserAgentSpecificMemory;
  if (!measure) return null;
  try {
    const sample = await measure.call(performance);
    return Number.isFinite(sample.bytes) ? sample.bytes : null;
  } catch {
    // Memory telemetry is optional and must never affect solver execution.
    return null;
  }
}

async function postProgress(event: Record<string, unknown>) {
  const now = performance.now();
  if (now - lastProgressPost < 5_000) return;
  lastProgressPost = now;
  try {
    await fetch('/__progress', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        elapsed_ms: now - started,
        repeat_index: samples.length + 1,
        event
      })
    });
  } catch {
    // Progress collection is benchmark-only best effort.
  }
}

function compactTerminalEvent(event: Record<string, unknown>) {
  const searchReport = event.search_report as Record<string, unknown> | undefined;
  if (!searchReport) return event;
  const {
    normalized_solution_keys: _normalizedSolutionKeys,
    packing_candidate_keys: _packingCandidateKeys,
    ...compactSearchReport
  } = searchReport;
  const setupReport = compactSearchReport.setup_report as
    | Record<string, unknown>
    | null
    | undefined;
  if (setupReport) {
    const conditions = Array.isArray(setupReport.hold_conditions)
      ? setupReport.hold_conditions.map((value) => {
          const condition = value as Record<string, unknown>;
          const candidates = Array.isArray(condition.candidates)
            ? condition.candidates as Array<Record<string, unknown>>
            : [];
          const { candidates: _candidates, ...summary } = condition;
          return {
            ...summary,
            first_candidate: candidates[0]
              ? {
                  setup_id: candidates[0].setup_id,
                  board_mask: candidates[0].board_mask,
                  min_locks: candidates[0].min_locks,
                  max_locks: candidates[0].max_locks,
                  build_covered_patterns: candidates[0].build_covered_patterns,
                  joint_covered_patterns: candidates[0].joint_covered_patterns,
                  build_probability: candidates[0].build_probability,
                  joint_probability: candidates[0].joint_probability,
                  conditional_pc_probability: candidates[0].conditional_pc_probability,
                  solution_path_count: candidates[0].solution_path_count,
                  solution_paths_complete: candidates[0].solution_paths_complete
                }
              : null
          };
        })
      : [];
    compactSearchReport.setup_report = {
      ...setupReport,
      hold_conditions: conditions
    };
  }
  return { ...event, search_report: compactSearchReport };
}

function browserCapabilities() {
  return {
    hardware_concurrency: navigator.hardwareConcurrency,
    webgpu: 'gpu' in navigator,
    cross_origin_isolated: crossOriginIsolated,
    user_agent: navigator.userAgent
  };
}

function boundedInteger(value: string | null, fallback: number, min: number, max: number) {
  if (value === null) return fallback;
  const parsed = Number(value);
  if (!/^\d+$/u.test(value) || !Number.isSafeInteger(parsed) || parsed < min || parsed > max) {
    throw new Error(`benchmark value must be an integer from ${min} to ${max}`);
  }
  return parsed;
}

function parseOptionalPositiveInteger(value: string | null): number | null {
  if (value === null) return null;
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed <= 0) {
    throw new Error('prewarmWorkers must be a positive integer');
  }
  return parsed;
}
