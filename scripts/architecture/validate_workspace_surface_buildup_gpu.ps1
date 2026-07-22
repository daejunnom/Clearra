# This file is dot-sourced by Invoke-WorkspaceSurfaceArchitectureValidation.
# It intentionally contains ordered validation statements, not a standalone entrypoint.

$uProblemHeader = Read-Text "core-c/include/clr_problem.h"
$uBuildupWorker = Read-Text "core-c/src/buildup/buildup_worker.c"
$uHoldQueueVerifier = Read-Text "core-c/src/buildup/hold_queue_verifier.c"
$uBuildupInternal = Read-Text "core-c/src/buildup/buildup_internal.h"
$uBuildupBfsState = Read-Text "core-c/src/buildup/buildup_bfs_state.h"
$uBuildupMemoKey = Read-Text "core-c/src/buildup/buildup_memo.c"
$uBuildupTests = Get-BuildUpTestsValidationSurface
$uCoreFfiNativeBuildup = Read-Text "crates/clearra-core-ffi/src/native/buildup.rs"
$uRawBindings = Read-Text "crates/clearra-core-ffi/src/raw/bindings.rs"
$uBuildupMode = Read-Text "crates/clearra-core-executor/src/buildup/buildup_execution_mode.rs"
$uRustBuildupMemoKey = Read-Text "crates/clearra-core-executor/src/buildup/buildup_memo_key.rs"
$uBuildupRunner = Read-Text "crates/clearra-core-executor/src/buildup/buildup_runner.rs"
$uBuildVariantReplayEvidence = Read-Text "crates/clearra-core-executor/src/spin/build_variant_replay_evidence.rs"
$uBuildVariantMapper = Read-Text "crates/clearra-core-executor/src/spin/build_variant_mapper.rs"
$uBuildupReplayBridge = Read-Text "crates/clearra-core-executor/src/buildup/buildup_replay_bridge.rs"
$uReplayEvent = Read-Text "crates/clearra-replay/src/replay/replay_event.rs"
$uReplayEngine = Read-Text "crates/clearra-replay/src/replay/replay_engine.rs"
$uFumenLikeWriter = Read-Text "crates/clearra-fumen/src/codec/fumen_like_writer.rs"
$uOutputFumenLikeBridge = Read-Text "crates/clearra-output/src/fumen_like/mod.rs"
$uReplayJsonContract = Read-Text "crates/clearra-output/src/json/replay_json_contract.rs"
$uSpinTargetRunner = Read-Text "crates/clearra-core-executor/src/spin/spin_target_runner.rs"
$uSpinTargetRunnerTests = Read-Text "crates/clearra-core-executor/src/spin/spin_target_runner_tests.rs"
$uSpinTargetReplaySurface = $uSpinTargetRunner + "`n" + $uBuildVariantReplayEvidence + "`n" + $uBuildVariantMapper
$uResultViews = Read-Text "crates/clearra-core-executor/src/result_views.rs"
$uProblemSpinCompiler = Read-Text "crates/clearra-problem/src/compile/spin_target_compiler.rs"
$uGpuTrustState = Read-Text "crates/clearra-core-executor/src/backend/gpu_trust_state.rs"
$uHybridBackpressure = Read-Text "crates/clearra-core-executor/src/backend/hybrid_backpressure_report.rs"
$uGpuWorkerSurface = @(
    Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/mod.rs"
    Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_request.rs"
    Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_result.rs"
    Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_result_reducer.rs"
    Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_backend_report.rs"
    Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_result_validation.rs"
    Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_exactness_gate.rs"
    Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_cpu_confirm_bridge.rs"
    Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_state.rs"
    Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_submission.rs"
    Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_error.rs"
    Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_memory_ticket.rs"
    Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_fence_epoch.rs"
    Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_backpressure.rs"
    Read-Text "crates/clearra-core-ffi/src/gpu/gpu_packing_batch_descriptor_view.rs"
    Read-Text "crates/clearra-core-ffi/src/gpu/gpu_worker_request_view.rs"
    Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/packing_batch_id.rs"
    Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/packing_batch_descriptor.rs"
    Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/packing_batch_descriptor_builder.rs"
    Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/packing_batch_validation.rs"
    Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_backend_capability.rs"
    Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_contract_tests.rs"
    Read-Text "crates/clearra-core-executor/src/backend/gpu_worker/gpu_worker_batch_descriptor_contract_tests.rs"
) -join "`n"
$uSchedulerHeader = Read-Text "core-c/src/scheduler/hybrid_scheduler.h"
$uScheduler = Read-Text "core-c/src/scheduler/hybrid_scheduler.c"
$uSchedulerBackpressure = Read-Text "core-c/src/scheduler/hybrid_backpressure.c"
$uSchedulerTests = Get-SchedulerTestsValidationSurface
foreach ($requiredMarker in @(
    "CLR_BUILDUP_MODE_VERIFY_FIRST",
    "CLR_BUILDUP_MODE_ENUMERATE_VARIANTS",
    "CLR_BUILDUP_MODE_COUNT_VARIANTS",
    "clr_buildup_verify_first",
    "clr_buildup_enumerate_variants",
    "clr_buildup_count_variants",
    "clr_buildup_enumeration_limits",
    "clr_buildup_count_report",
    "hold_branch_kind"
)) {
    if ($uProblemHeader -notlike "*$requiredMarker*") {
        Add-ArchitectureError "BuildUp C ABI must split verify/enumerate/count marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "clearra_buildup_queue_hold_enumerate_branches",
    "ClearraBuildUpHoldBranch",
    "CLEARRA_BUILDUP_HOLD_BRANCH_CURRENT",
    "CLEARRA_BUILDUP_HOLD_BRANCH_SWAP_HELD",
    "CLEARRA_BUILDUP_HOLD_BRANCH_STORE_CURRENT"
)) {
    $surface = "$uBuildupInternal`n$uHoldQueueVerifier"
    if ($surface -notlike "*$requiredMarker*") {
        Add-ArchitectureError "BuildUp hold automaton must preserve enumerate-mode branch marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "clr_piece_source_pattern_piece_at",
    "clearra_hold_automaton_apply",
    "refresh_bag_state_from_reader",
    "bag_remainder_key_with_piece",
    "clearra_buildup_verify_bag_pattern"
)) {
    if ($uHoldQueueVerifier -notlike "*$requiredMarker*") {
        Add-ArchitectureError "BuildUp hold/queue verifier must use PieceSource reader and refresh bag automaton state marker '$requiredMarker'"
    }
}
foreach ($forbiddenMarker in @(
    "piece_multiset_window.counts[piece]",
    "queue_piece_at",
    "synthetic_queue"
)) {
    if ($uHoldQueueVerifier -like "*$forbiddenMarker*") {
        Add-ArchitectureError "BuildUp hold/queue verifier must not reconstruct a sorted multiset pseudo-queue marker '$forbiddenMarker'"
    }
}
foreach ($requiredMarker in @(
    "buildup_modes_split_verify_first_enumerate_and_count",
    "buildup_verify_first_returns_single_witness",
    "buildup_enumerate_variants_returns_expected_count_for_two_operation_fixture",
    "buildup_count_variants_matches_enumerate_variants_for_small_fixture",
    "buildup_count_variants_reports_total_count_without_retaining_traces",
    "enumerate_variants_truncates_after_limit_without_losing_prefix",
    "enumerate_variants_preserves_hold_branches",
    "buildup_enumerate_variants_preserves_hold_branches",
    "buildup_enumerate_variants_returns_multiple_hold_branches",
    "buildup_enumerate_variants_preserves_hold_branch_kind",
    "enumerate_variants_always_preserves_hold_branches",
    "coverage_mode_never_calls_consume_first_branch_only",
    "build_variant_exports_hold_branch_kind",
    "build_variant_exports_kick_evidence",
    "fixed_queue_tio_not_reordered_to_iot",
    "fixed_queue_order_not_reordered_by_multiset",
    "fixed_queue_same_multiset_different_order_changes_buildability",
    "same_multiset_different_queue_changes_buildability",
    "hold_disabled_uses_actual_queue_order",
    "hold_enabled_long_carryover_uses_piece_source_pattern",
    "long_hold_carryover_uses_bag_epoch_and_remainder",
    "bag_universe_pattern_id_controls_sequence",
    "bag_pattern_id_changes_hold_reachable_language",
    "materialized_pattern_reader_preserves_pattern_order",
    "piece_source_reader_rejects_provenance_mismatch",
    "hold_transition_updates_bag_epoch_and_remainder_from_piece_source_pattern",
    "bag_universe_allows_duplicate_across_bag_epoch",
    "synthetic_multiset_queue_is_forbidden",
    "count_variants_reports_complete_count_without_retaining_all_traces",
    "EXPECT_U64(variants->count, 2)",
    "EXPECT_U64(report.total_variant_count, variants->count)",
    "EXPECT_U64(variants->count, 120)",
    "EXPECT_U64(count_report.total_variant_count, 120)"
)) {
    if ($uBuildupTests -notlike "*$requiredMarker*") {
        Add-ArchitectureError "C BuildUp tests must cover mode/hold/count marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "CNativeBuildUpEnumerationLimits",
    "CNativeBuildUpCountLimits",
    "CNativeBuildUpCountReport",
    "hold_branch_kind",
    "enumerate_buildup_variants",
    "count_buildup_variants",
    "verify_first_buildup_problem",
    "buildup_enumeration_limits_preserve_variant_budget"
)) {
    $surface = "$uCoreFfiNativeBuildup`n$uRawBindings"
    if ($surface -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-core-ffi must expose split BuildUp native API marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "BuildUpExecutionMode",
    "VerifyFirst",
    "EnumerateVariants",
    "CountVariants",
    "coverage_producing",
    "can_source_min_cover",
    "verify_first_result_is_not_used_for_coverage",
    "verify_first_result_not_used_for_min_cover",
    "min_cover_never_uses_verify_first"
)) {
    if ($uBuildupMode -notlike "*$requiredMarker*") {
        Add-ArchitectureError "Rust BuildUpExecutionMode must guard coverage source marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "BuildUpExecutionMode::coverage_producing",
    "enumerate_buildup_variants",
    "buildup_enumeration_limits",
    "c_max_variants_from_budget",
    "buildup_enumerate_variant_limit_comes_from_problem_budget",
    "coverage_source",
    "enumerate-variants"
)) {
    if ($uBuildupRunner -notlike "*$requiredMarker*") {
        Add-ArchitectureError "BuildUpRunner must use enumerate variants as coverage source marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "BuildVariantReplayEvidence",
    "BuildVariantReplayEvidenceError",
    "from_native_build_variant_and_candidate",
    "replay_operations_from_candidate",
    "MissingOperationBasis",
    "SpinTargetPredicate::new",
    "BuildVariantMapper::to_replay_trace",
    "c-build-variant-operation-replay-basis",
    "replay_evidence.operations().is_empty()",
    "with_representative_order",
    "step_index_for_kick_evidence",
    "replay_kick_evidence",
    "scoring_kick_evidence",
    "replay_trace_completeness",
    "SpinTargetCoverageBridge::row_from_build_variant",
    "spin_target_predicate_applies_after_replay_before_coverage_row",
    "kick_evidence_flows_from_build_variant_to_spin_classifier",
    "missing_kick_evidence_is_incomplete_not_exact_spin",
    "tsd_probability_threshold_query_reports_satisfaction",
    "threshold_satisfied"
)) {
    if ($uSpinTargetReplaySurface -notlike "*$requiredMarker*") {
        Add-ArchitectureError "SpinTargetRunner must apply predicate after replay and support threshold marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "spin_target_runner_uses_all_build_variant_operations",
    "spin_target_runner_preserves_variant_board_before_after",
    "native_build_variant_to_replay_trace_uses_operation_set_from_candidate",
    "spin_target_runner_rejects_missing_operation_basis",
    "missing_operation_basis_is_error",
    "spin_target_runner_uses_real_kick_evidence_count",
    "spin_target_runner_rejects_missing_spin_basis_for_exact_query",
    "spin_target_runner_does_not_use_stub_t_operation_for_native_variant",
    "spin_target_runner_rejects_stub_replay_basis"
)) {
    if ($uSpinTargetRunnerTests -notlike "*$requiredMarker*") {
        Add-ArchitectureError "SpinTargetRunner tests must guard real replay reconstruction marker '$requiredMarker'"
    }
}
foreach ($forbiddenMarker in @(
    "portable_reference_buildup_fallback_allowed",
    "portable_reference_buildup_witness",
    "fallback_build_variant_from_candidate"
)) {
    if ($uBuildupRunner -like "*$forbiddenMarker*") {
        Add-ArchitectureError "BuildUp product execution must not expose fixture or portable fallback marker '$forbiddenMarker'"
    }
}
foreach ($requiredMarker in @(
    "clr_buildup_bfs_state",
    "remaining_ops_bitset",
    "current_board_mask",
    "clr_deleted_line_state",
    "clr_hold_automaton_state",
    "piece_source_cursor",
    "reachability_relevant_state"
)) {
    if ($uBuildupBfsState -notlike "*$requiredMarker*") {
        Add-ArchitectureError "BuildUp BFS state must include full operation-order search field '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "clearra_buildup_memo_key_from_bfs_state",
    "clearra_buildup_memo_key_hash",
    "clearra_buildup_hold_automaton_memo_key_hash",
    "deleted_line_state",
    "hold_automaton_state_hash",
    "reachability_relevant_state"
)) {
    if ($uBuildupMemoKey -notlike "*$requiredMarker*") {
        Add-ArchitectureError "BuildUp memo key must include full BFS state marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "ReplayEvent",
    "Placement",
    "HoldStore",
    "HoldSwap",
    "LineClear",
    "KickEvidence",
    "SpinBasis",
    "ScoreBasis",
    "BoardSnapshot",
    "TraceCompleteness"
)) {
    if ($uReplayEvent -notlike "*$requiredMarker*") {
        Add-ArchitectureError "Replay event vocabulary must include M9 marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "buildup_state_contains_deleted_line_state",
    "buildup_state_contains_hold_automaton_state",
    "buildup_memo_key_differs_by_deleted_line_state",
    "buildup_memo_key_differs_by_bag_epoch",
    "buildup_memo_key_differs_by_bag_remainder_key",
    "buildup_memo_key_differs_by_reachability_state"
)) {
    if ($uBuildupTests -notlike "*$requiredMarker*") {
        Add-ArchitectureError "C BuildUp tests must cover BFS state/memo marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "BuildVariantReplayInput",
    "representative_order",
    "ReplayScoreBasisEvent::new",
    "ReplayBoardSnapshotEvent::new",
    "replay_trace_preserves_line_clear_events",
    "rust_replay_event_preserves_kick_evidence"
)) {
    if ($uReplayEngine -notlike "*$requiredMarker*") {
        Add-ArchitectureError "ReplayEngine must build replay traces from BuildVariant input marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "BuildUpMemoKey",
    "DeletedLineState",
    "hold_automaton_state_hash",
    "piece_source_cursor",
    "reachability_relevant_state",
    "buildup_memo_key_differs_by_bag_epoch",
    "buildup_memo_key_differs_by_bag_remainder_key"
)) {
    if ($uRustBuildupMemoKey -notlike "*$requiredMarker*") {
        Add-ArchitectureError "Rust BuildUp memo key must mirror full BFS memo marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "native_replay_trace_from_build_variant",
    "BuildVariantReplayEvidence::from_native_build_variant_and_candidate",
    "CBuildVariantView::from_native",
    "BuildVariantMapper::to_replay_trace_with_marker"
)) {
    if ($uBuildupReplayBridge -notlike "*$requiredMarker*") {
        Add-ArchitectureError "BuildUp replay bridge must route native sample traces through BuildVariant evidence marker '$requiredMarker'"
    }
}
foreach ($forbiddenMarker in @(
    "native_replay_trace_from_candidates",
    "native-candidate-"
)) {
    if ($uBuildupReplayBridge -like "*$forbiddenMarker*") {
        Add-ArchitectureError "BuildUp replay bridge must not synthesize replay directly from PackingCandidate marker '$forbiddenMarker'"
    }
}
foreach ($requiredMarker in @(
    "FumenLikeWriter::write_replay_trace",
    "replay_event_page",
    "fumen_writer_consumes_replay_trace_events_not_core_candidate",
    "type=kick-evidence",
    "type=score-basis",
    "type=board-snapshot"
)) {
    if ($uFumenLikeWriter -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-fumen writer must consume ReplayTrace events for fumen-like output marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "clearra-fumen",
    "pub use clearra_fumen::codec",
    "FumenLikeWriter"
)) {
    if ($uOutputFumenLikeBridge -notlike "*$requiredMarker*") {
        Add-ArchitectureError "clearra-output fumen_like bridge must re-export clearra-fumen codec marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "hold-store",
    "hold-swap",
    "score-basis",
    "board-snapshot",
    "board_snapshot_phase_name"
)) {
    if ($uReplayJsonContract -notlike "*$requiredMarker*") {
        Add-ArchitectureError "Replay JSON contract must expose M9 replay event marker '$requiredMarker'"
    }
}
foreach ($forbiddenMarker in @(
    "standard_10_by_lines(4)",
    "c-build-variant-minimal-replay-basis",
    "BuildVariantOperation::new(PieceKind::T, RotationState::Zero, 0, 0)"
)) {
    if ($uSpinTargetRunner -like "*$forbiddenMarker*") {
        Add-ArchitectureError "SpinTargetRunner must not synthesize stub replay marker '$forbiddenMarker'"
    }
}
foreach ($requiredMarker in @(
    "percent_spin_target_query_compiles_to_search_problem",
    "setup_spin_target_query_preserves_threshold",
    "pc_then_spin_compiles_to_composite_goal",
    "spin_target_query_requires_score_profile_when_profile_specific"
)) {
    if ($uProblemSpinCompiler -notlike "*$requiredMarker*") {
        Add-ArchitectureError "SpinTarget compiler must support percent/setup/pc threshold query marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "score_event_basis",
    "kick_evidence_count",
    "build_variant_view_exposes_score_event_basis_and_kick_evidence_count"
)) {
    if ($uResultViews -notlike "*$requiredMarker*") {
        Add-ArchitectureError "BuildVariantView must expose score event basis and KickEvidence marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "GpuTrustState",
    "GpuComputedUnconfirmed",
    "GpuComputedCpuConfirmed",
    "DeterministicReferenceMatched",
    "can_source_exact_probability",
    "gpu_trust_state_requires_cpu_confirm_for_exact_probability"
)) {
    if ($uGpuTrustState -notlike "*$requiredMarker*") {
        Add-ArchitectureError "GpuTrustState must guard trusted GPU result marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "HybridBackpressureReport",
    "HybridThrottleReason",
    "throttle_reason",
    "hybrid_backpressure_reports_throttle_reason"
)) {
    if ($uHybridBackpressure -notlike "*$requiredMarker*") {
        Add-ArchitectureError "Rust hybrid backpressure report must expose marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "GpuWorkerRequest",
    "GpuWorkerResult",
    "GpuWorkerState",
    "GpuWorkerSubmission",
    "GpuWorkerError",
    "GpuMemoryTicket",
    "GpuFenceEpoch",
    "GpuWorkerBackpressure",
    "GpuBackendCapability",
    "native_gpu_backend_not_built",
    "DeterministicReferenceMatched",
    "GpuComputedUnconfirmed",
    "can_source_exact_probability",
    "cpu_confirm_required",
    "memory_ticket_id",
    "gpu_worker_unconfirmed_result_cannot_source_exact_probability",
    "gpu_worker_cpu_confirmed_result_can_source_exact_probability",
    "gpu_memory_ticket_preserves_scope_epoch_and_budget",
    "gpu_memory_ticket_rejects_missing_ticket_epoch_or_budget",
    "gpu_worker_backpressure_reports_throttle_reason",
    "gpu_worker_fallback_result_carries_reason",
    "InvalidMemoryTicket",
    "PackingBatchDescriptor",
    "PackingBatchDescriptorBuilder",
    "PackingBatchId",
    "GpuWorkerRequest",
    "PackingBatchValidationError",
    "packing_batch_descriptor_rejects_zero_piece_count",
    "packing_batch_descriptor_rejects_board_over_board64_limit",
    "packing_batch_descriptor_preserves_pattern_universe_identity",
    "packing_batch_descriptor_preserves_rule_and_kick_profile_id",
    "packing_batch_descriptor_builder_uses_problem_budget",
    "gpu_worker_request_preserves_batch_descriptor",
    "gpu_worker_request_requires_memory_ticket",
    "gpu_worker_request_requires_cpu_confirm_for_real_gpu",
    "CGpuPackingBatchDescriptorView",
    "CGpuWorkerRequestView",
    "to_c_descriptor_view",
    "rust_gpu_batch_descriptor_maps_to_c_descriptor",
    "c_gpu_batch_descriptor_preserves_pattern_universe_id",
    "c_gpu_batch_descriptor_preserves_weight_model_id",
    "gpu_batch_descriptor_abi_size_is_stable",
    "gpu_batch_descriptor_rejects_unsupported_board_shape",
    "GpuWorkerResultReducer",
    "GpuWorkerReduction",
    "GpuWorkerBackendReport",
    "GpuWorkerExactnessGate",
    "validate_gpu_worker_result",
    "PrefilterOnly",
    "ExactCandidateSource",
    "RejectedMismatch",
    "gpu_unconfirmed_result_reduces_to_prefilter_only",
    "gpu_cpu_confirmed_result_reduces_to_exact_candidate_source",
    "gpu_deterministic_reference_result_reduces_to_exact_candidate_source",
    "gpu_fallback_result_reduces_to_backend_fallback_report",
    "gpu_mismatch_result_is_rejected",
    "GpuCpuConfirmBridge",
    "GpuCpuConfirmBridgeDecision",
    "GpuCpuConfirmBridgeError",
    "can_enter_cpu_buildup_queue",
    "can_create_coverage_row",
    "candidate_is_solution",
    "gpu_confirmed_candidate_can_enter_cpu_buildup_queue",
    "gpu_unconfirmed_candidate_cannot_create_coverage_row"
)) {
    if ($uGpuWorkerSurface -notlike "*$requiredMarker*") {
        Add-ArchitectureError "Rust GPU worker v0.1 must expose contract marker '$requiredMarker'"
    }
}
foreach ($requiredMarker in @(
    "ClearraHybridBackpressureReport",
    "ClearraHybridThrottleReason",
    "clearra_hybrid_backpressure_report_for",
    "hybrid_backpressure_reports_throttle_reason",
    "gpu_worker_request_submitted",
    "cpu_buildup_backlog",
    "coverage_row_buffer_pressure",
    "hybrid_scheduler_submits_gpu_worker_request",
    "hybrid_scheduler_reports_gpu_queue_depth",
    "hybrid_scheduler_reports_readback_pending",
    "hybrid_scheduler_throttles_when_cpu_buildup_backlog_high",
    "hybrid_scheduler_throttles_when_coverage_buffer_pressure_high",
    "hybrid_scheduler_fallback_reports_reason"
)) {
    $surface = "$uSchedulerHeader`n$uScheduler`n$uSchedulerBackpressure`n$uSchedulerTests"
    if ($surface -notlike "*$requiredMarker*") {
        Add-ArchitectureError "C hybrid scheduler must report backpressure marker '$requiredMarker'"
    }
}
