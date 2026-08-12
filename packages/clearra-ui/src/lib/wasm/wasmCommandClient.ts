import type { RenderCapabilityReport } from '../render/renderCapabilityReport';
import type { ClearraWasmForcedTerminationReason } from './wasmWorkerLifecycle';

export type ClearraVirtualFileHandle = {
  handle_id: string;
  display_name: string;
  mime_type: string;
  byte_len: number;
  origin_kind: 'browser-file-input';
};

export type ClearraWasmCommandRequest = {
  commandText: string;
  virtualFiles?: ClearraVirtualFileHandle[];
};

export type ClearraDiagnostic = {
  code: string;
  severity: string;
  message: string;
};

export type ClearraDiagnosticReport = {
  diagnostics: ClearraDiagnostic[];
};

export type ClearraHostAppResponse = {
  command: string | null;
  status: 'success' | 'validation-failed' | 'unsupported' | 'execution-failed';
  result: { kind: string } | null;
  diagnostics: ClearraDiagnostic[];
  backend_report: {
    backend_requested: string;
    backend_selected: string;
    fallback_used: boolean;
    fallback_reason: string | null;
    backend_fallback_reason: string | null;
    fallback_backend: string | null;
    gpu_failure_class: string | null;
    gpu_failure_stage: string | null;
    discarded_partial_gpu_result: boolean;
    gpu_device_requested: string | null;
    gpu_device_selected_index: number | null;
    gpu_device_selected_name: string | null;
    gpu_device_selected_type: string | null;
    gpu_device_selected_backend: string | null;
  };
  resource_report: {
    solver_executed: boolean;
    memory_status: string;
    truncated: boolean;
    truncation_reason: string | null;
    peak_frontier_states: number;
    peak_candidate_rows: number;
    peak_hash_buckets: number;
    peak_gpu_bytes: number;
    peak_cpu_bytes: number;
    build_worker_backlog_peak: number;
    coverage_rows_emitted: number;
    probability_complete: boolean;
  };
  capability_report: {
    app_request_boundary: string;
    executor_boundary: string;
    render_capability: RenderCapabilityReport;
  };
  continuation: { available: boolean; token: string | null } | null;
};

export type ClearraWebGpuLimitsReport = {
  max_storage_buffer_binding_size: number;
  max_compute_workgroup_storage_size: number;
  max_compute_invocations_per_workgroup: number;
};

export type ClearraWebGpuBackendReport = {
  outcome_state: 'NotRequested' | 'Connected' | 'Unavailable';
  webgpu_available: boolean;
  webgpu_adapter_label_or_redacted: string;
  webgpu_limits: ClearraWebGpuLimitsReport;
  webgpu_required_limits: ClearraWebGpuLimitsReport;
  webgpu_unavailable_reason: string | null;
  expected_digest: string | null;
  actual_digest: string | null;
  shader: {
    shader_compile_status: string;
    shader_hash: string | null;
    shader_version: string | null;
    embedded_reviewed: boolean;
    user_shader_allowed: boolean;
    runtime_shader_injection_allowed: boolean;
  };
  memory: { wasm_memory_usage: string; wasm_memory_pressure: string };
  fallback_used: boolean;
  fallback_backend: string | null;
  gpu_warmup_requested: boolean;
  gpu_warmup_performed: boolean;
  gpu_session_reused: boolean;
  gpu_trust_state: 'NotUsed' | 'TrustedCpuSampleConfirmed' | 'Unavailable';
  cpu_confirmed: boolean;
  can_source_exact_probability: boolean;
};

export type ClearraBudgetStatus = {
  state: string;
  used: number;
  limit: number | null;
};

export type ClearraBackendStatus = {
  backend_requested: string;
  backend_selected: string;
  fallback_used: boolean;
  fallback_reason: string | null;
};

export type ClearraMemoryStatus = {
  state: string;
  raw_pointer_exposed: boolean;
};

export type ClearraWasmSearchPathStep = {
  piece: string;
  rotation: number;
  x: number;
  y: number;
  hold: string;
  cleared_lines: number;
};

export type ClearraForwardPathStep = {
  piece: string;
  rotation: number;
  x: number;
  y: number;
  hold: string;
  cleared_lines: number;
  spin_piece: string | null;
  spin_mini: boolean;
  damage: number;
  total_damage: number;
  placement_mask: string;
  cleared_row_mask: number;
  board_after_mask: string;
};

export type ClearraForwardSearchOutcome = {
  id: string;
  source_pattern_index: number;
  source_queue: string;
  group: 't' | 'other' | 'integrated' | null;
  final_board_mask: string;
  spin_piece: string | null;
  spin_mini: boolean;
  spin_lines: number;
  total_damage: number;
  path: ClearraForwardPathStep[];
};

export type ClearraSolutionProbability = {
  solution_key: string;
  probability: string;
  covered_pattern_count: number;
  pattern_count: number;
  probability_complete: boolean;
};

export type ClearraSolutionAverageScore = {
  solution_key: string;
  average_score: string;
  covered_pattern_count: number;
  pattern_count: number;
  score_complete: boolean;
};

export type ClearraSetupCandidate = {
  setup_id: string;
  board_mask: string;
  min_locks: number;
  max_locks: number;
  build_covered_patterns: number;
  joint_covered_patterns: number;
  build_probability: string;
  joint_probability: string;
  conditional_pc_probability: string;
  representative_path: ClearraWasmSearchPathStep[];
  solution_path_count?: number;
  solution_paths_complete?: boolean;
  solution_paths?: ClearraWasmSearchPathStep[][];
};

export type ClearraSetupHoldCondition = {
  condition_id: string;
  initial_hold: string | null;
  pattern_expression: string;
  pattern_count: number;
  candidate_count: number;
  result_truncated: boolean;
  complete: boolean;
  candidates: ClearraSetupCandidate[];
};

export type ClearraSetupFinderReport = {
  search_mode: 'oracle' | 'qb';
  cycle: number;
  remaining_pieces: string;
  queue_based_pieces: string;
  next_cycle_remaining_pieces: string;
  post_cycle_borrow_enabled: boolean;
  coverage_semantics: 'full-future-oracle' | 'visible-seven-policy';
  continuation_supply_semantics: 'exact-post-setup-hold-queue-state';
  geometry_family_count: string;
  partial_build_node_count: number;
  complete: boolean;
  hold_conditions: ClearraSetupHoldCondition[];
};

export type ClearraFinesseSolutionAverage = {
  solution_key: string;
  average_inputs: string;
  complete: boolean;
};

export type ClearraFinessePolicyResult = {
  policy: 'oracle' | 'visible-7';
  overall_average_inputs: string;
  complete: boolean;
  oracle_on_covered_average_inputs?: string | null;
  information_penalty_inputs?: string | null;
  success_probability_gap?: string | null;
  successful_probability_mass?: string | null;
  successful_unique_queue_count?: number | null;
  total_unique_queue_count?: number | null;
  solution_averages: ClearraFinesseSolutionAverage[];
};

export type ClearraFinesseReportInput =
  | 'hold'
  | 'tap-left'
  | 'tap-right'
  | 'das-left'
  | 'das-right'
  | 'rotate-clockwise'
  | 'rotate-counter-clockwise'
  | 'rotate-180'
  | 'soft-drop'
  | 'hard-drop';

export type ClearraFinesseRepresentativeWitness = {
  policy: 'oracle' | 'visible-7';
  solution_key?: string | null;
  pattern_ids: number[];
  queue: string[];
  total_inputs: number;
  input_sequence: ClearraFinesseReportInput[];
  placements: ClearraFinessePlacement[];
};

export type ClearraFinessePlacement = {
  piece: string;
  rotation: number;
  x: number;
  y: number;
};

export type ClearraFinesseReport = {
  metric: 'inputs';
  mode: 'score' | 'search';
  pattern_knowledge: 'both' | 'oracle' | 'visible-7';
  complete: boolean;
  exact_total_inputs?: string | number | null;
  representative_witness?: ClearraFinesseRepresentativeWitness | null;
  policy_results: ClearraFinessePolicyResult[];
};

export type ClearraWasmSearchReport = {
  backend_selected: string;
  workers_used: number;
  cpu_parallel_execution: boolean;
  cpu_parallel_decision_reason: string;
  solution_found: boolean;
  packing_candidate_count: number;
  packing_candidate_set_digest: string;
  packing_candidate_keys: string[];
  unique_solution_count: number;
  solution_count_calculated: boolean;
  solution_set_materialized: boolean;
  solution_keys_materialized_count: number;
  solution_keys_complete: boolean;
  solution_page_available: boolean;
  normalized_solution_set_hash: string;
  normalized_solution_keys: string[];
  solution_probabilities: ClearraSolutionProbability[];
  solution_average_scores: ClearraSolutionAverageScore[];
  build_variant_count: number;
  build_variant_count_exact: string;
  buildability_verified: boolean;
  coverage_calculated: boolean;
  probability_calculated: boolean;
  materialized_pattern_count: number;
  covered_pattern_count: number;
  coverage_probability: string;
  probability_complete: boolean;
  count_complete: boolean;
  searched_nodes: number;
  peak_frontier_states: number;
  peak_cpu_bytes: number;
  representative_candidate_id: string | null;
  representative_pattern_id: number | null;
  representative_path: ClearraWasmSearchPathStep[];
  summary_fields: Array<[string, string]>;
  forward_search_kind: 'damage' | 'spin-finder' | null;
  forward_initial_board_mask: string | null;
  maximum_damage: number | null;
  forward_outcomes: ClearraForwardSearchOutcome[];
  setup_report: ClearraSetupFinderReport | null;
  finesse_report?: ClearraFinesseReport | null;
};

type ClearraWasmWorkerEventBase = {
  schema_version: 1;
  runtime: 'clearra-wasm';
  job_id: number;
};

export type ClearraSearchProgressTelemetry = {
  phase:
    | 'preparing'
    | 'initializing'
    | 'searching'
    | 'draining'
    | 'postprocessing'
    | 'merging';
  producer_complete: boolean;
  geometry_nodes: number;
  candidates_emitted: number;
  geometry_family_count: string | null;
  candidates_verified: number;
  producer_build_nodes: number;
  producer_coverage_checks: number;
  build_nodes: number;
  coverage_checks: number;
  ready_workers: number;
  active_workers: number;
  worker_count: number;
  oldest_batch_ms: number;
  pass_index: number;
  pass_count: number;
  layer_index: number;
  layer_count: number;
  layer_done: number;
  layer_total: number;
};

export type ClearraWasmWorkerEvent = ClearraWasmWorkerEventBase &
  (
    | { event: 'started' }
    | {
        event: 'progress';
        progress: {
          done: number;
          total: number;
          label: string;
          budget_status: ClearraBudgetStatus;
          backend_status: ClearraBackendStatus;
          memory_status: ClearraMemoryStatus;
          telemetry?: ClearraSearchProgressTelemetry;
        };
      }
    | { event: 'diagnostic'; diagnostic: ClearraDiagnostic }
    | { event: 'partial_result'; partial: boolean; label: string; final_result: boolean }
    | {
        event: 'final_response';
        response: ClearraHostAppResponse;
        webgpu_backend: ClearraWebGpuBackendReport;
        search_report: ClearraWasmSearchReport | null;
      }
    | { event: 'failed'; diagnostics: ClearraDiagnosticReport }
    | { event: 'cancelled'; scope_released: boolean }
    | {
        event: 'terminated';
        reason: ClearraWasmForcedTerminationReason;
        scope_released: true;
        diagnostics: ClearraDiagnosticReport;
      }
  );

export type ClearraSolutionPageWorkerEvent =
  | {
      type: 'solution_page';
      request_id: number;
      offset: number;
      total: number;
      keys: string[];
    }
  | {
      type: 'solution_page_failed';
      request_id: number;
      message: string;
    };

export function buildWasmCommandRequest(
  input: Partial<ClearraWasmCommandRequest>
): ClearraWasmCommandRequest {
  return {
    commandText: input.commandText ?? 'clearra verify kicks',
    virtualFiles: input.virtualFiles ?? []
  };
}

export function createBrowserVirtualFileHandle(file: File): ClearraVirtualFileHandle {
  return {
    handle_id: crypto.randomUUID(),
    display_name: file.name,
    mime_type: file.type || 'application/octet-stream',
    byte_len: file.size,
    origin_kind: 'browser-file-input'
  };
}

export function postRunCommand(
  worker: Worker,
  request: ClearraWasmCommandRequest,
  prewarmWorkerCount = 1,
  tablebaseRequested = false,
  lifecycleOwnerId?: string
) {
  worker.postMessage({
    type: 'run_command_text',
    commandText: request.commandText,
    prewarmWorkerCount,
    tablebaseRequested,
    lifecycleOwnerId,
    virtualFiles: request.virtualFiles ?? []
  });
}

export function postPrewarmRuntime(
  worker: Worker,
  workerCount: number,
  tablebaseRequested = false,
  lifecycleOwnerId?: string
) {
  worker.postMessage({
    type: 'prewarm_runtime',
    workerCount,
    tablebaseRequested,
    lifecycleOwnerId
  });
}

export function postCancelJob(worker: Worker, jobId?: number) {
  worker.postMessage(jobId === undefined ? { type: 'cancel_job' } : { type: 'cancel_job', jobId });
}

export function postLoadSolutionPage(
  worker: Worker,
  requestId: number,
  offset: number,
  limit: number
) {
  worker.postMessage({
    type: 'load_solution_page',
    requestId,
    offset,
    limit
  });
}

export function isSolutionPageWorkerEvent(
  value: unknown
): value is ClearraSolutionPageWorkerEvent {
  if (!value || typeof value !== 'object') return false;
  const type = (value as { type?: unknown }).type;
  return type === 'solution_page' || type === 'solution_page_failed';
}
