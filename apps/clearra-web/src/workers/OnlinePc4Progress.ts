import type { ClearraWasmWorkerEvent } from '@clearra/ui/wasm';

// Network requests are not Geometry nodes or a prediction of all solutions.
// Keep this observation separate from core candidate/coverage counters.
export function onlinePc4Progress(jobId: number, requests: number): ClearraWasmWorkerEvent {
  return {
    schema_version: 1, runtime: 'clearra-wasm', event: 'progress', job_id: jobId,
    progress: {
      done: requests, total: 0, label: 'pc4-online',
      budget_status: { state: 'within-budget', used: 0, limit: null },
      backend_status: { backend_requested: 'cpu', backend_selected: 'wasm-cpu',
        fallback_used: false, fallback_reason: null },
      memory_status: { state: 'wasm-computation-scope-active', raw_pointer_exposed: false }
    }
  };
}
