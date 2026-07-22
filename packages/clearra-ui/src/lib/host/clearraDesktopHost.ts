import { invoke } from '@tauri-apps/api/core';

import type { RenderCapabilityReport } from '../render/renderCapabilityReport';

export type ClearraDesktopRequest = {
  app_request_model: 'clearra-app/AppRequest';
  command: 'pc' | 'pc-scenario';
  language: 'en' | 'ko';
  lines: number;
  queue: string;
  hold_enabled: boolean;
  hold_piece: 'empty' | 'I' | 'O' | 'T' | 'S' | 'Z' | 'J' | 'L';
  backend: 'auto' | 'cpu' | 'gpu' | 'hybrid';
  rule: 'srs-plus' | string;
  board_mask: string;
  visible_height: number;
  piece_window: number | null;
  count_policy: 'unique' | 'all';
  solution_probabilities: boolean;
  workers: number;
  gpu_device: string;
  allow_backend_fallback: boolean;
  memory_budget_mb: number;
  candidate_budget: number;
  pattern_budget: number;
};

export type ClearraDesktopAppResponse = {
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
  scope_released?: boolean;
  backend_status?: ClearraDesktopBackendStatus;
  memory_status?: ClearraDesktopMemoryStatus;
  resource_status?: ClearraDesktopResourceStatus;
};

export function buildDesktopAppRequest(
  input: Partial<ClearraDesktopRequest>
): ClearraDesktopRequest {
  return {
    app_request_model: 'clearra-app/AppRequest',
    command: 'pc',
    language: input.language ?? 'en',
    lines: input.lines ?? 2,
    queue: input.queue ?? '',
    hold_enabled: input.hold_enabled ?? false,
    hold_piece: input.hold_piece ?? 'empty',
    backend: input.backend ?? 'auto',
    rule: input.rule ?? 'srs-plus',
    board_mask: input.board_mask ?? '0x0000000000000000',
    visible_height: input.visible_height ?? input.lines ?? 2,
    piece_window: input.piece_window ?? null,
    count_policy: input.count_policy ?? 'unique',
    solution_probabilities: input.solution_probabilities ?? false,
    workers: input.workers ?? 0,
    gpu_device: input.gpu_device ?? 'auto',
    allow_backend_fallback: input.allow_backend_fallback ?? true,
    memory_budget_mb: input.memory_budget_mb ?? 1024,
    candidate_budget: input.candidate_budget ?? 10_000_000,
    pattern_budget: input.pattern_budget ?? 5040
  };
}

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
