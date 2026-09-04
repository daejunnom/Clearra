import { invoke } from '@tauri-apps/api/core';

import type { RenderCapabilityReport } from '../render/renderCapabilityReport';
import type {
  ExecutionAvailabilityReport,
  ExecutionCompletenessState
} from '../wasm/executionAvailability';
import type {
  ClearraProductPageWorkerPayload,
  ClearraProductResultPayload,
  ClearraProductBuildIdentity,
  ClearraWasmSearchReport
} from '../wasm/wasmCommandClient';

/**
 * The sole production request accepted by the native desktop transport.
 * Every GUI workspace supplies the complete canonical argv consumed by the
 * shared CLI compiler; neither this client nor the Rust host synthesizes
 * product defaults from GUI-only fields.
 */
export type ClearraDesktopCliCommandRequest = {
  app_request_model: 'clearra-cli/CommandRequest';
  command: 'cli';
  language: 'en' | 'ko';
  arguments: string[];
};

export type ClearraDesktopRequest = ClearraDesktopCliCommandRequest;

export type ClearraDesktopAppResponse = {
  runtime_identity: ClearraProductBuildIdentity;
  status: 'success' | 'validation-failed' | 'unsupported' | 'execution-failed';
  diagnostics: Array<{ code: string; severity: string; message: string }>;
  product_result_payload?: ClearraProductResultPayload | null;
  solution_set_artifact?: import('../wasm/wasmCommandClient').ClearraSolutionSetArtifactPayload | null;
  capability_report: {
    app_request_boundary: string;
    executor_boundary: string;
    render_capability: RenderCapabilityReport;
  };
  backend_report?: ClearraDesktopBackendStatus;
  resource_report?: ClearraDesktopResourceStatus;
  [key: string]: unknown;
};

export type ClearraDesktopBackendStatus = {
  backend_requested?: string;
  backend_selected?: string;
  fallback_used?: boolean;
  backend_fallback_reason?: string | null;
};

export type ClearraDesktopMemoryStatus = {
  state?: string;
  leak_report_clean?: boolean;
  raw_pointer_exposed?: boolean;
};

export type ClearraDesktopResourceStatus = {
  budget_status?: string;
  done?: number;
  total?: number;
  truncated?: boolean;
  truncation_reason?: string | null;
  probability_complete?: boolean;
  execution_availability?: ExecutionAvailabilityReport;
  result_completeness?: ExecutionCompletenessState;
};

export type ClearraDesktopJobEvent = {
  schema_version: 1;
  event: 'started' | 'progress' | 'diagnostic' | 'completed' | 'failed' | 'cancelled';
  job_id: number;
  done?: number;
  total?: number;
  label?: string;
  code?: string;
  severity?: string;
  response?: ClearraDesktopAppResponse;
  search_report?: ClearraWasmSearchReport | null;
  scope_released?: boolean;
  backend_status?: ClearraDesktopBackendStatus;
  memory_status?: ClearraDesktopMemoryStatus;
  resource_status?: ClearraDesktopResourceStatus;
};

export async function runRequest(
  request: ClearraDesktopRequest
): Promise<ClearraDesktopAppResponse> {
  const response = await invoke<string>('run_request', { requestJson: JSON.stringify(request) });
  return JSON.parse(response) as ClearraDesktopAppResponse;
}

export async function validateRequest(request: ClearraDesktopRequest): Promise<unknown> {
  const response = await invoke<string>('validate_request', {
    requestJson: JSON.stringify(request)
  });
  return JSON.parse(response);
}

export async function startJob(request: ClearraDesktopRequest): Promise<number> {
  return await invoke<number>('start_job', { requestJson: JSON.stringify(request) });
}

export async function cancelJob(jobId: number): Promise<void> {
  await invoke<void>('cancel_job', { jobId });
}

export async function getJobEvents(jobId: number): Promise<ClearraDesktopJobEvent[]> {
  const response = await invoke<string>('get_job_events', { jobId });
  return JSON.parse(response) as ClearraDesktopJobEvent[];
}

export async function loadNextProductPage(
  maximumWorkSteps = 10_000,
  signal?: AbortSignal
): Promise<ClearraProductPageWorkerPayload> {
  if (signal?.aborted) throw abortError(signal);
  const releaseOnAbort = () => {
    void releaseProductPages().catch(() => undefined);
  };
  signal?.addEventListener('abort', releaseOnAbort, { once: true });
  try {
    if (signal?.aborted) {
      releaseOnAbort();
      throw abortError(signal);
    }
    const response = await invoke<string>('product_page_next', {
      maximumWorkSteps
    });
    if (signal?.aborted) throw abortError(signal);
    return JSON.parse(response) as ClearraProductPageWorkerPayload;
  } catch (error) {
    if (signal?.aborted) throw abortError(signal);
    throw error;
  } finally {
    signal?.removeEventListener('abort', releaseOnAbort);
  }
}

export async function loadProductMemberPage(
  alternativeIndex: string,
  memberPageNumber: string,
  signal?: AbortSignal
): Promise<ClearraProductPageWorkerPayload> {
  if (signal?.aborted) throw abortError(signal);
  requireCanonicalProductPageCoordinate(alternativeIndex, 'alternative index');
  requireCanonicalProductPageCoordinate(memberPageNumber, 'member page number');
  const releaseOnAbort = () => {
    void releaseProductPages().catch(() => undefined);
  };
  signal?.addEventListener('abort', releaseOnAbort, { once: true });
  try {
    if (signal?.aborted) {
      releaseOnAbort();
      throw abortError(signal);
    }
    const response = await invoke<string>('product_page_get', {
      alternativeIndex,
      memberPageNumber
    });
    if (signal?.aborted) throw abortError(signal);
    return JSON.parse(response) as ClearraProductPageWorkerPayload;
  } catch (error) {
    if (signal?.aborted) throw abortError(signal);
    throw error;
  } finally {
    signal?.removeEventListener('abort', releaseOnAbort);
  }
}

function requireCanonicalProductPageCoordinate(value: string, label: string): void {
  if (!/^[1-9][0-9]*$/u.test(value)) {
    throw new Error(`${label} must be a canonical positive decimal string`);
  }
}

export async function releaseProductPages(): Promise<void> {
  await invoke<void>('product_page_release');
}

function abortError(signal: AbortSignal): Error {
  if (signal.reason instanceof Error) return signal.reason;
  const error = new Error('Product page load was aborted.');
  error.name = 'AbortError';
  return error;
}

export type ClearraGpuWarmupReport = {
  state: 'connected' | 'unavailable';
  device_index: number | null;
  device_name: string | null;
  unavailable_reason: string | null;
  session_cached: boolean;
  session_reused: boolean;
  initialization_elapsed_ns: number;
};

export async function prewarmSearchBackend(
  gpuDevice: number | null = null
): Promise<ClearraGpuWarmupReport> {
  const response = await invoke<string>('prewarm_search_backend', { gpuDevice });
  return JSON.parse(response) as ClearraGpuWarmupReport;
}
