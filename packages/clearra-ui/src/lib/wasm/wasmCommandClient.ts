// SRP rationale: this module has one change reason: the typed UI client contract for WASM command execution.
import type { RenderCapabilityReport } from '../render/renderCapabilityReport';
import type {
  HostCapabilitySnapshot,
  RuntimeWarmupPolicy,
  WorkerAuthorityReport
} from './hostCapabilitySnapshot';
import type { ClearraWasmForcedTerminationReason } from './wasmWorkerLifecycle';
import type {
  ExecutionAvailabilityReport,
  ExecutionCompletenessState
} from './executionAvailability';

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

export type ClearraProductBuildIdentity = {
  engine_build_id: string;
  source_commit: string;
  contract_schema_version: 'clearra.search.contract.v2';
  supply_semantics_id: 'clearra.supply.projected-terminal-lookahead.v1';
  artifact_schema_version: 'clearra.solution-data.v1';
};

export type ClearraProductCandidateMemberPayload = {
  candidate_id: string;
  normalized_solution_key: string;
};

export type ClearraCoveragePortfolioPagePayload = {
  set_contract: string;
  page_contract: string;
  member_page_contract: string;
  set_identity_sha256: string;
  candidate_map_sha256: string;
  alternative_index: string;
  optimal_cardinality: string;
  known_alternative_count: string;
  total_alternative_count: string | null;
  enumeration_complete: boolean;
  member_page_number: string;
  total_member_pages: string;
  members: ClearraProductCandidateMemberPayload[];
  page_handle_available: boolean;
};

export type ClearraBuildV2CompletenessPayload = {
  input_identity_bound: boolean;
  producer_filter_bound: boolean;
  buildability_replay_complete: boolean;
  coverage_rows_complete: boolean;
  probability_weights_complete: boolean;
  exact_minimum_proven: boolean;
  score_evidence_complete: boolean;
};

export type ClearraBuildV2CandidateCoveragePayload = {
  candidate_key: string;
  covered_pattern_count: string;
};

export type ClearraBuildV2ScoreWinnerPayload = {
  pattern_id: string;
  candidate_key: string;
  score: string;
  informational_attack: string;
};

export type ClearraBuildV2ProductPayload = {
  kind: 'candidate-family' | 'probability' | 'portfolio' | 'score-portfolio';
  capability_id: string;
  result_contract: string;
  input_identity_sha256: string;
  evaluation_identity_sha256: string | null;
  replay_basis: string | null;
  objective: 'all' | 'unique' | 'min-cover' | 'max-probability-minimum' | 'max-score-cover';
  score_profile: 'tetrio' | 'guideline' | 'jstris-ultra' | null;
  initial_b2b: string | null;
  score_accuracy: string | null;
  profile_specific_exact: boolean | null;
  score_equality_basis: 'score-only' | null;
  informational_attack_basis: 'canonical-equal-score-trace' | null;
  source_candidate_count: string;
  reachable_candidate_count: string;
  selected_candidate_count: string | null;
  pattern_count: string;
  covered_pattern_count: string | null;
  required_pattern_count: string | null;
  union_probability: string | null;
  b2b_preservation_required: boolean | null;
  candidates: ClearraBuildV2CandidateCoveragePayload[];
  canonical_candidate_keys: string[];
  winners: ClearraBuildV2ScoreWinnerPayload[];
  completeness: ClearraBuildV2CompletenessPayload;
  page_source_available: boolean;
  page_source_identity_sha256: string | null;
};

export type ClearraBuildCoveragePortfolioV2Payload = {
  contract: 'build-coverage-portfolio.v2';
  objective: 'min-cover' | 'max-probability-minimum';
  probability_basis: string;
  source_candidate_count: string;
  selected_candidate_count: string;
  pattern_count: string;
  required_pattern_count: string;
  union_probability: string;
  normalized_solution_set_hash: string;
  canonical_first_candidate_id: string;
  completeness: {
    source_universe_complete: boolean;
    coverage_rows_complete: boolean;
    probability_weights_complete: boolean;
    exact_minimum_proven: boolean;
    query_bound: boolean;
  };
  page_source_available: boolean;
  page_source_identity_sha256: string | null;
};

export type ClearraBuildSetupFamilyV1Payload = {
  contract: 'build-target-family.v2';
  input_identity_sha256: string;
  evaluation_identity_sha256: string;
  objective: 'all' | 'unique';
  source_candidate_count: string;
  reachable_candidate_count: string;
  pattern_count: string;
  covered_pattern_count: string;
  union_probability: string;
  completeness: {
    input_identity_bound: boolean;
    producer_filter_bound: boolean;
    buildability_replay_complete: boolean;
    coverage_rows_complete: boolean;
    probability_weights_complete: boolean;
  };
  candidates: ClearraBuildV2CandidateCoveragePayload[];
};

export type ClearraSetupRankedCandidatePayload = {
  candidate_id: string;
  condition_id: string;
  setup_id: string;
};

export type ClearraSetupRankedFamilyPayload = {
  schema_id:
    | 'setup-joint-ranking.v2'
    | 'setup-build-ranking.v2'
    | 'setup-pc-ranking.v2';
  query_identity_sha256: string;
  rule_profile: string;
  supply_identity_sha256: string;
  universe_identity_sha256: string;
  product_build: string;
  ordering:
    | 'joint-probability-descending'
    | 'build-probability-descending'
    | 'conditional-pc-probability-descending';
  resolved_length_preference: 'longer' | 'shorter';
  candidate_count: string;
  candidates: ClearraSetupRankedCandidatePayload[];
};

export type ClearraSetupScoreCandidatePayload = {
  rank: string;
  candidate_id: string;
  completed_board_mask: string;
  setup_covered_pattern_count: string;
  setup_covered_probability: string;
  continuation_probability: string;
  unconditional_expected_score: string;
};

export type ClearraSetupScoreRankingPayload = {
  schema_id: 'setup-score-ranking.v1';
  input_identity_sha256: string;
  evaluation_identity_sha256: string;
  document_format: 'ctk3' | 'fumen';
  rule_profile: string;
  score_profile: 'tetrio' | 'guideline' | 'jstris-ultra';
  initial_b2b: string;
  ordering: 'unconditional-expected-score-descending-then-canonical-candidate-id';
  source_page_count: string;
  candidate_count: string;
  setup_pattern_count: string;
  average_priority_score: string;
  complete: true;
  candidates: ClearraSetupScoreCandidatePayload[];
};

export type ClearraSpinStructureCandidatePayload = {
  candidate_id: string;
  partition: 'regular' | 'mini';
  placement_count: string;
};

export type ClearraSpinStructureFamilyPayload = {
  schema_id: 'spin-structure-family.v2' | 'spin-structure-guaranteed.v1';
  query_identity_sha256: string;
  rule_profile: string;
  spin_profile: string;
  supply_identity_sha256: string;
  universe_identity_sha256: string;
  product_build: string;
  ordering: 'regular-then-mini-canonical-operation-key';
  minimum_placements: string | null;
  guaranteed_final_piece: 'I' | 'O' | 'T' | 'S' | 'Z' | 'J' | 'L' | null;
  guarantee_basis:
    | 'every-unique-non-target-piece-order-exact-replay-final-piece-last'
    | null;
  dependency_report_included: boolean | null;
  dependency_relation: 'non-target-universal-precedence' | null;
  dependency_edge_count: string | null;
  regular_count: string;
  mini_count: string;
  candidate_count: string;
  complete: boolean;
  candidates: ClearraSpinStructureCandidatePayload[];
};

export type ClearraSolutionSetArtifactFormatPayload = {
  format: 'ctk3' | 'fumen';
  state: 'available' | 'unavailable';
  unavailable_reason:
    | 'empty-solution-set'
    | 'unsupported-solution-key'
    | 'page-limit-exceeded'
    | 'encoding-failed'
    | 'transport-byte-limit-exceeded'
    | null;
  media_type: string | null;
  filename: string | null;
  byte_length: number | null;
  sha256: string | null;
  page_count: number | null;
  document: string | null;
};

export type ClearraSolutionSetArtifactPayload = {
  contract: 'solution-set-artifact.v2';
  source_result_kind: string;
  source_solution_set_contract: string;
  selection_kind: 'solution-family' | 'portfolio-alternative' | 'canonical-result';
  selection_id: string;
  page_source_identity_sha256: string | null;
  normalized_key_algorithm: string;
  normalized_set_hash_algorithm: string;
  normalized_set_hash: string;
  solution_count: number;
  completeness: 'complete';
  formats: [ClearraSolutionSetArtifactFormatPayload, ClearraSolutionSetArtifactFormatPayload];
};

export type ClearraScorePatternWinnerPayload = {
  pattern_id: string;
  candidate_id: string;
  normalized_solution_key: string;
  score: string;
  informational_attack: string;
};

export type ClearraPcScoreFieldPayload = {
  normalized_field_key: string;
  average_score: string;
  covered_pattern_count: string;
  pattern_count: string;
  score_complete: boolean;
};

export type ClearraPcScoreFieldSummaryPayload = {
  field_contract: 'pc-score-solution-field-average.v1';
  ordering: 'normalized-solution-field-order';
  solution_field_average_basis: 'whole-materialized-pattern-universe-failed-pc-zero';
  score_evaluation_basis: 'all-traces';
  score_evaluation_scope: 'full';
  overall_score_basis: 'all-materialized-patterns-failed-pc-zero';
  piece_source_id: string;
  pattern_universe_id: string;
  pattern_weight_model_id: string;
  materialized_pattern_count: string;
  solution_field_count: string;
  scored_pattern_count: string;
  failed_pc_pattern_count: string;
  covered_probability: string;
  overall_score: string;
  score_covered_pattern_conditional_average_score: string | null;
  complete: true;
  fields: ClearraPcScoreFieldPayload[];
};

export type ClearraPcPathStepPayload = {
  step_index: string;
  operation_id: string;
  active_piece: string;
  input_cursor: string;
  output_cursor: string;
  input_hold_piece: string | null;
  output_hold_piece: string | null;
  hold_decision: string;
  rotation: string;
  x: string;
  y: string;
  placement_mask: string;
  board_before_mask: string;
  board_after_placement_mask: string;
  board_after_line_clear_mask: string;
  cleared_row_mask: string;
  cleared_lines: string;
  line_clear_identity: string;
};

export type ClearraPcPathWitnessPayload = {
  candidate_id: string;
  producer_candidate_id: string;
  pattern_id: string;
  trace_identity: string;
  normalized_trace_key: string;
  consumed_piece_count: string;
  terminal_hold_piece: string | null;
  steps: ClearraPcPathStepPayload[];
};

export type ClearraPcPathFamilyPayload = {
  witness_contract: 'pc-path-witness.v2';
  ordering: 'candidate-id-ascending-then-pattern-id-ascending-then-trace-key-ascending';
  problem_id: string;
  materialized_pattern_count: string;
  witness_count: string;
  complete: true;
  witnesses: ClearraPcPathWitnessPayload[];
};

export type ClearraParityReportPagePayload = {
  document_format: 'ctk3' | 'fumen';
  page_number: number;
  total_pages: number;
  coordinate_basis: string;
  width: number;
  height: number;
  occupied_cell_count: number;
  checker_black_count: number;
  checker_white_count: number;
  checker_delta: number;
  four_color_counts: [number, number, number, number];
  even_column_count: number;
  odd_column_count: number;
  column_parity_delta: number;
  occupied_area_mod_four: number;
  pending_garbage_occupied_cell_count: number;
  feasibility_claim: false;
  pruning_authority: 'none';
  page_handle_available: boolean;
};

export type ClearraFieldDocumentPayload = {
  format: 'ctk3' | 'fumen';
  document: string;
  page_count: number;
  canonical_sha256: string;
  filename: string;
};

export type ClearraFieldDocumentSetPayload = {
  document_contract: string;
  documents: ClearraFieldDocumentPayload[];
};

export type ClearraRenderArtifactPayload = {
  document_format: 'ctk3' | 'fumen';
  artifact_format: 'png' | 'gif';
  selected_page_number: number | null;
  document_page_count: number;
  media_type: 'image/png' | 'image/gif';
  filename: string;
  byte_length: number;
  sha256: string;
  bytes_base64: string;
  render_exact: true;
  skin_id: string;
  product_max_bytes: number;
  transport_max_bytes: number;
};

export type ClearraProductResultPayload =
  | {
      contract: string;
      result_kind: string;
      content: {
        payload_kind: 'build-v2';
        payload: ClearraBuildV2ProductPayload;
      };
    }
  | {
      contract: 'build.cover';
      result_kind: 'build-coverage-portfolio.v2';
      content: {
        payload_kind: 'build-coverage-portfolio-v2';
        payload: ClearraBuildCoveragePortfolioV2Payload;
      };
    }
  | {
      contract: 'build.setup';
      result_kind: 'build-target-family.v2';
      content: {
        payload_kind: 'build-setup-family-v1';
        payload: ClearraBuildSetupFamilyV1Payload;
      };
    }
  | {
      contract: 'setup.joint' | 'setup.build' | 'setup.pc';
      result_kind:
        | 'setup-joint-ranking.v2'
        | 'setup-build-ranking.v2'
        | 'setup-pc-ranking.v2';
      content: {
        payload_kind: 'setup-ranked-family';
        payload: ClearraSetupRankedFamilyPayload;
      };
    }
  | {
      contract: 'setup.score';
      result_kind: 'setup-score-ranking.v1';
      content: {
        payload_kind: 'setup-score-ranking';
        payload: ClearraSetupScoreRankingPayload;
      };
    }
  | {
      contract: 'spin-structure.search';
      result_kind: 'spin-structure-family.v2';
      content: {
        payload_kind: 'spin-structure-family';
        payload: ClearraSpinStructureFamilyPayload;
      };
    }
  | {
      contract: 'spin-structure.guaranteed';
      result_kind: 'spin-structure-guaranteed.v1';
      content: {
        payload_kind: 'spin-structure-family';
        payload: ClearraSpinStructureFamilyPayload;
      };
    }
  | {
      contract: 'spin-structure.cover';
      result_kind: 'spin-structure-coverage.v1';
      content: {
        payload_kind: 'coverage-portfolio';
        payload: ClearraCoveragePortfolioPagePayload;
      };
    }
  | {
      contract: 'pc.minimals';
      result_kind: 'pc-minimum-cover.v2';
      content: {
        payload_kind: 'coverage-portfolio';
        payload: ClearraCoveragePortfolioPagePayload;
      };
    }
  | {
      contract: 'pc.score-minimals';
      result_kind: 'pc-score-portfolio.v2';
      content: {
        payload_kind: 'coverage-portfolio';
        payload: ClearraCoveragePortfolioPagePayload;
      };
    }
  | {
      contract: 'pc.path';
      result_kind: 'pc-path-family.v2';
      content: {
        payload_kind: 'pc-path-family';
        payload: ClearraPcPathFamilyPayload;
      };
    }
  | {
      contract: 'pc.score';
      result_kind: 'pc-score-summary.v2';
      content: {
        payload_kind: 'pc-score-field-summary';
        payload: ClearraPcScoreFieldSummaryPayload;
      };
    }
  | {
      contract: 'pc.score-finder';
      result_kind: 'pc-fixed-score-witness.v2';
      content: {
        payload_kind: 'score-pattern-winner-family';
        payload: {
          winner_contract: string;
          ordering: 'pattern-id-ascending-then-candidate-id-ascending';
          equality: 'score-only-attack-informational';
          informational_attack_basis: string;
          page_size: string;
          winner_count: string;
          winners: ClearraScorePatternWinnerPayload[];
        };
      };
    }
  | {
      contract: string;
      result_kind: string;
      content: {
        payload_kind: 'parity-report-page';
        payload: ClearraParityReportPagePayload;
      };
    }
  | {
      contract: string;
      result_kind: string;
      content: {
        payload_kind: 'field-document';
        payload: ClearraFieldDocumentPayload;
      };
    }
  | {
      contract: string;
      result_kind: string;
      content: {
        payload_kind: 'field-document-set';
        payload: ClearraFieldDocumentSetPayload;
      };
    }
  | {
      contract: string;
      result_kind: string;
      content: {
        payload_kind: 'render-artifact';
        payload: ClearraRenderArtifactPayload;
      };
    };

export type ClearraCoveragePortfolioRuntimePage = Omit<
  ClearraCoveragePortfolioPagePayload,
  'set_contract' | 'page_handle_available'
>;

export type ClearraProductPageWorkerPayload =
  | {
      schema_version: 1;
      runtime: 'clearra-wasm' | 'clearra-desktop';
      product_page_kind: 'coverage-portfolio';
      state: 'page';
      page: ClearraCoveragePortfolioRuntimePage;
    }
  | {
      schema_version: 1;
      runtime: 'clearra-wasm' | 'clearra-desktop';
      product_page_kind: 'coverage-portfolio';
      state: 'work-budget-exhausted' | 'cancelled' | 'sealed';
      known_alternative_count: string;
      enumeration_complete: boolean;
    }
  | {
      schema_version: 1;
      runtime: 'clearra-wasm' | 'clearra-desktop';
      product_page_kind: 'parity-report';
      state: 'page';
      page: ClearraParityReportPagePayload;
    }
  | {
      schema_version: 1;
      runtime: 'clearra-wasm' | 'clearra-desktop';
      product_page_kind: 'parity-report';
      state: 'exhausted';
    };

export type ClearraHostAppResponse = {
  runtime_identity: ClearraProductBuildIdentity;
  command: string | null;
  status: 'success' | 'validation-failed' | 'unsupported' | 'execution-failed';
  result: { kind: string } | null;
  product_result_payload?: ClearraProductResultPayload | null;
  solution_set_artifact?: ClearraSolutionSetArtifactPayload | null;
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
    execution_availability: ExecutionAvailabilityReport;
    result_completeness: ExecutionCompletenessState;
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
  ren_count: number | null;
  total_damage: number;
  evidence_path_count: string;
  evidence_complete: boolean;
  path: ClearraForwardPathStep[];
};

export type ClearraSolutionProbability = {
  solution_key: string;
  probability: string;
  covered_pattern_count: number;
  pattern_count: number;
  probability_complete: boolean;
};

export type ClearraWasmRuntimeAuthority = {
  hostCapabilitySnapshot: HostCapabilitySnapshot;
  workerAuthority: WorkerAuthorityReport;
  warmupPolicy: RuntimeWarmupPolicy;
};

export type ClearraSolutionAverageScore = {
  solution_key: string;
  average_score: string;
  covered_pattern_count: number;
  pattern_count: number;
  score_complete: boolean;
};

export type ClearraSetupCandidate = {
  candidate_id: string;
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

export type ClearraSpinStructureOperation = {
  piece: string;
  rotation: number;
  x: number;
  y: number;
  logical_mask: string;
  need_deleted_rows: number;
};

export type ClearraSpinStructureOutcome = {
  candidate_id: string;
  partition: 'regular' | 'mini';
  placement_count: number;
  board_before_spin: string;
  final_board: string;
  cleared_lines: number;
  logical_spin_cleared_rows: number;
  logical_spin: ClearraSpinStructureOperation;
  logical_operations: ClearraSpinStructureOperation[];
};

export type ClearraSpinStructureReport = {
  initial_board_mask: string;
  height: number;
  inventory: string;
  spin_profile: string;
  line_requirement: string;
  fill_bottom: number;
  fill_top: number;
  rule_profile: string;
  minimality: string;
  minimum_placements: number | null;
  workers_used: number;
  complete: boolean;
  regular: ClearraSpinStructureOutcome[];
  mini: ClearraSpinStructureOutcome[];
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
  supply_window_resolution: string;
  projects_unplaced_lookahead: boolean;
  projects_standard_bag_lookahead: boolean;
  source_sequence_length: number;
  total_possible_pattern_count: string;
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
  forward_search_kind: 'damage' | 'spin-finder' | 'ren' | null;
  forward_initial_board_mask: string | null;
  maximum_damage: number | null;
  maximum_ren: number | null;
  forward_outcomes: ClearraForwardSearchOutcome[];
  setup_report: ClearraSetupFinderReport | null;
  spin_structure_report: ClearraSpinStructureReport | null;
  finesse_report?: ClearraFinesseReport | null;
};

type ClearraWasmWorkerEventBase = {
  schema_version: 1;
  runtime: 'clearra-wasm';
  job_id: number;
};

export type ClearraSearchProgressTelemetry = {
  /**
   * Runtime-selected execution path. The browser owner uses this explicit
   * authority marker to keep a serial-search watchdog separate from worker
   * startup/prewarm and from the distributed verifier watchdog.
   */
  execution_mode?: 'serial' | 'distributed';
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
  availability: ClearraSearchProgressTelemetryFlags;
  exactness: ClearraSearchProgressTelemetryFlags;
};

export type ClearraSearchProgressCountKey =
  | 'geometry_nodes'
  | 'candidates_emitted'
  | 'geometry_family_count'
  | 'candidates_verified'
  | 'producer_build_nodes'
  | 'producer_coverage_checks'
  | 'build_nodes'
  | 'coverage_checks'
  | 'ready_workers'
  | 'active_workers'
  | 'worker_count'
  | 'oldest_batch_ms'
  | 'pass_index'
  | 'pass_count'
  | 'layer_index'
  | 'layer_count'
  | 'layer_done'
  | 'layer_total';

export type ClearraSearchProgressTelemetryFlags = Record<
  ClearraSearchProgressCountKey,
  boolean
>;

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
    | {
        event: 'failed';
        diagnostics: ClearraDiagnosticReport;
        response?: ClearraHostAppResponse | null;
        resource_report?: ClearraHostAppResponse['resource_report'];
        execution_availability?: ExecutionAvailabilityReport;
        result_completeness?: ExecutionCompletenessState;
      }
    | {
        event: 'cancelled';
        scope_released: boolean;
        execution_availability?: ExecutionAvailabilityReport;
        result_completeness?: ExecutionCompletenessState;
      }
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

export type ClearraProductPageWorkerEvent =
  | {
      type: 'product_page';
      request_id: number;
      payload: ClearraProductPageWorkerPayload;
    }
  | {
      type: 'product_page_failed';
      request_id: number;
      message: string;
    };

export function buildWasmCommandRequest(
  input: Partial<ClearraWasmCommandRequest>
): ClearraWasmCommandRequest {
  return {
    commandText: input.commandText ?? 'clearra pc path --lines 4 --queue IOTSZ',
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
  lifecycleOwnerId?: string,
  runtimeAuthority?: ClearraWasmRuntimeAuthority
) {
  worker.postMessage({
    type: 'run_command_text',
    commandText: request.commandText,
    prewarmWorkerCount,
    tablebaseRequested,
    lifecycleOwnerId,
    ...runtimeAuthority,
    virtualFiles: request.virtualFiles ?? []
  });
}

export function postPrewarmRuntime(
  worker: Worker,
  workerCount: number,
  tablebaseRequested = false,
  lifecycleOwnerId?: string,
  runtimeAuthority?: ClearraWasmRuntimeAuthority
) {
  worker.postMessage({
    type: 'prewarm_runtime',
    workerCount,
    tablebaseRequested,
    lifecycleOwnerId,
    ...runtimeAuthority
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

export function postLoadNextProductPage(
  worker: Worker,
  requestId: number,
  maximumWorkSteps = 10_000
) {
  worker.postMessage({
    type: 'load_product_page',
    requestId,
    action: 'next',
    maximumWorkSteps
  });
}

export function postLoadProductMemberPage(
  worker: Worker,
  requestId: number,
  alternativeIndex: string,
  memberPageNumber: string
) {
  worker.postMessage({
    type: 'load_product_page',
    requestId,
    action: 'get',
    alternativeIndex,
    memberPageNumber
  });
}

export function postReleaseProductPages(worker: Worker) {
  worker.postMessage({ type: 'release_product_pages' });
}

export function isSolutionPageWorkerEvent(
  value: unknown
): value is ClearraSolutionPageWorkerEvent {
  if (!value || typeof value !== 'object') return false;
  const type = (value as { type?: unknown }).type;
  return type === 'solution_page' || type === 'solution_page_failed';
}

export function isProductPageWorkerEvent(
  value: unknown
): value is ClearraProductPageWorkerEvent {
  if (!value || typeof value !== 'object') return false;
  const type = (value as { type?: unknown }).type;
  return type === 'product_page' || type === 'product_page_failed';
}
