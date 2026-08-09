import { invoke } from '@tauri-apps/api/core';

import type { RenderCapabilityReport } from '../render/renderCapabilityReport';
import type { ClearraWasmSearchReport } from '../wasm/wasmCommandClient';

export type ClearraDesktopRequest = {
  app_request_model: 'clearra-app/AppRequest';
  command: 'pc' | 'pc-scenario' | 'setup' | 'build-probability' | 'damage' | 'spin-finder';
  language: 'en' | 'ko';
  lines: number;
  queue: string;
  patterns: string;
  queue_knowledge: 'oracle' | 'visible-7';
  hold_enabled: boolean;
  hold_piece: 'empty' | 'I' | 'O' | 'T' | 'S' | 'Z' | 'J' | 'L';
  backend: 'auto' | 'cpu' | 'gpu' | 'hybrid';
  rule: 'srs-plus' | string;
  score_mode: 'tiling' | 'off' | 'minimum-cover' | 'summary' | 'failed-queue';
  score_profile: 'guideline' | 'jstris-ultra' | 'tetrio';
  spin_profile:
    | 't-spins'
    | 't-spins-plus'
    | 'all-spin'
    | 'all-spin-plus'
    | 'all-mini'
    | 'all-mini-plus';
  preserve_b2b: boolean;
  precompute_build_dependencies: boolean;
  finesse: 'off' | 'inputs';
  pattern_knowledge: 'both' | 'oracle' | 'visible-7';
  initial_b2b: number;
  board_mask: string;
  visible_height: number;
  piece_window: number | null;
  count_policy: 'unique' | 'all';
  solution_probabilities: boolean;
  workers: number;
  use_all_logical_processors: boolean;
  gpu_device: string;
  allow_backend_fallback: boolean;
  /** Zero leaves memory unbounded by policy; allocator/host OOM still applies. */
  memory_budget_mb: number;
  candidate_budget: number;
  pattern_budget: number;
  tablebase_requested?: boolean;
  setup_mode?: 'oracle' | 'qb';
  setup_remaining?: string;
  setup_qb?: string;
  setup_next_cycle_remaining?: string;
  setup_allow_post_cycle_borrow?: boolean;
  setup_priority?: 'all' | 'build' | 'pc';
  setup_length?: 'auto' | 'longer' | 'shorter';
  setup_max_pieces?: number;
  setup_path_setup_id?: string;
  setup_path_condition_id?: string;
  base_mask?: string;
  target_mask?: string;
  build_aggregation?: 'buildability' | 'tiling' | 'spin';
  include_horizontal_mirror?: boolean;
  initial_combo?: number;
  damage_aggregation?: 'maximum' | 'at-least';
  minimum_damage?: number;
  spin_lines?: 'any' | '0' | '1' | '2' | '3' | '4' | '1+' | '2+' | '3+' | '4+';
  spin_category?: 'any' | 't' | 'other';
};

export type ClearraDesktopAppResponse = {
  status: 'success' | 'validation-failed' | 'unsupported' | 'execution-failed';
  diagnostics: Array<{ code: string; severity: string; message: string }>;
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
  search_report?: ClearraWasmSearchReport | null;
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
    command: input.command ?? 'pc',
    language: input.language ?? 'en',
    lines: input.lines ?? 2,
    queue: input.queue ?? '',
    patterns: input.patterns ?? '',
    queue_knowledge: input.queue_knowledge ?? 'oracle',
    hold_enabled: input.hold_enabled ?? true,
    hold_piece: input.hold_piece ?? 'empty',
    backend: input.backend ?? 'auto',
    rule: input.rule ?? 'srs-plus',
    score_mode: input.score_mode ?? 'off',
    score_profile: input.score_profile ?? 'tetrio',
    spin_profile: input.spin_profile ?? 't-spins',
    preserve_b2b: input.preserve_b2b ?? false,
    precompute_build_dependencies: input.precompute_build_dependencies ?? false,
    finesse: input.finesse ?? 'off',
    pattern_knowledge: input.pattern_knowledge ?? 'both',
    initial_b2b: input.initial_b2b ?? 0,
    board_mask: input.board_mask ?? '0x0000000000000000',
    visible_height: input.visible_height ?? input.lines ?? 2,
    piece_window: input.piece_window ?? null,
    count_policy: input.count_policy ?? 'unique',
    solution_probabilities: input.solution_probabilities ?? false,
    workers: input.workers ?? 0,
    use_all_logical_processors: input.use_all_logical_processors ?? false,
    gpu_device: input.gpu_device ?? 'auto',
    allow_backend_fallback: input.allow_backend_fallback ?? true,
    memory_budget_mb: input.memory_budget_mb ?? 0,
    candidate_budget: input.candidate_budget ?? 10_000_000,
    pattern_budget: input.pattern_budget ?? 5040,
    tablebase_requested: input.tablebase_requested ?? false,
    setup_mode: input.setup_mode ?? 'oracle',
    setup_remaining: input.setup_remaining ?? 'IOTSZJL',
    setup_qb: input.setup_qb ?? '',
    setup_next_cycle_remaining: input.setup_next_cycle_remaining ?? '',
    setup_allow_post_cycle_borrow: input.setup_allow_post_cycle_borrow ?? false,
    setup_priority: input.setup_priority ?? 'all',
    setup_length: input.setup_length ?? 'auto',
    setup_max_pieces: input.setup_max_pieces ?? 9,
    setup_path_setup_id: input.setup_path_setup_id,
    setup_path_condition_id: input.setup_path_condition_id,
    base_mask: input.base_mask ?? '0x0',
    target_mask: input.target_mask ?? '0x0',
    build_aggregation: input.build_aggregation ?? 'buildability',
    include_horizontal_mirror: input.include_horizontal_mirror ?? true,
    initial_combo: input.initial_combo ?? 0,
    damage_aggregation: input.damage_aggregation ?? 'maximum',
    minimum_damage: input.minimum_damage ?? 0,
    spin_lines: input.spin_lines ?? 'any',
    spin_category: input.spin_category ?? 'any'
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
