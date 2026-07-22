const params = new URLSearchParams(location.search);
const commandText = params.get('command');
const status = document.querySelector<HTMLPreElement>('#status');

if (!commandText || !status) throw new Error('benchmark command is required');

const started = performance.now();
const worker = new Worker(
  new URL('../../../apps/clearra-web/src/workers/clearraWorker.ts', import.meta.url),
  { type: 'module' }
);
let completed = false;
let peakBrowserMemoryBytes: number | null = null;
let memorySampleCount = 0;
let memorySamplePending = false;
let lastProgressPost = 0;
const memoryTimer = setInterval(() => void sampleBrowserMemory(), 1_000);
void sampleBrowserMemory();

worker.onmessage = (message: MessageEvent<Record<string, unknown>>) => {
  const event = message.data;
  if (event.event === 'progress') {
    void postProgress(event);
    return;
  }
  if (!['final_response', 'failed', 'cancelled'].includes(String(event.event))) return;
  const elapsedMs = performance.now() - started;
  void complete({
    surface: 'browser-wasm-product-worker',
    elapsed_ms: elapsedMs,
    browser_memory_peak_bytes: peakBrowserMemoryBytes,
    browser_memory_sample_count: memorySampleCount,
    capabilities: {
      hardware_concurrency: navigator.hardwareConcurrency,
      webgpu: 'gpu' in navigator,
      cross_origin_isolated: crossOriginIsolated,
      user_agent: navigator.userAgent
    },
    command: commandText,
    event: compactTerminalEvent(event)
  });
};

worker.onerror = (event) => {
  const elapsedMs = performance.now() - started;
  void complete({
    surface: 'browser-wasm-product-worker',
    elapsed_ms: elapsedMs,
    browser_memory_peak_bytes: peakBrowserMemoryBytes,
    browser_memory_sample_count: memorySampleCount,
    capabilities: {
      hardware_concurrency: navigator.hardwareConcurrency,
      webgpu: 'gpu' in navigator,
      cross_origin_isolated: crossOriginIsolated,
      user_agent: navigator.userAgent
    },
    command: commandText,
    worker_error: event.message
  });
};

worker.postMessage({ type: 'run_command_text', commandText });

async function complete(result: Record<string, unknown>) {
  if (completed) return;
  completed = true;
  clearInterval(memoryTimer);
  await sampleBrowserMemory();
  result.browser_memory_peak_bytes = peakBrowserMemoryBytes;
  result.browser_memory_sample_count = memorySampleCount;
  worker.terminate();
  status!.textContent = JSON.stringify(result, null, 2);
  const response = await fetch('/__result', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(result)
  });
  if (!response.ok) throw new Error(`benchmark collector rejected result: ${response.status}`);
  document.title = 'Clearra WASM benchmark complete';
}

async function sampleBrowserMemory() {
  if (memorySamplePending) return;
  const measure = (
    performance as Performance & {
      measureUserAgentSpecificMemory?: () => Promise<{ bytes: number }>;
    }
  ).measureUserAgentSpecificMemory;
  if (!measure) return;
  memorySamplePending = true;
  try {
    const sample = await measure.call(performance);
    if (Number.isFinite(sample.bytes)) {
      peakBrowserMemoryBytes = Math.max(peakBrowserMemoryBytes ?? 0, sample.bytes);
      memorySampleCount += 1;
    }
  } catch {
    // Memory telemetry is optional and must never affect solver execution.
  } finally {
    memorySamplePending = false;
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
        browser_memory_peak_bytes: peakBrowserMemoryBytes,
        browser_memory_sample_count: memorySampleCount,
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
  return { ...event, search_report: compactSearchReport };
}
