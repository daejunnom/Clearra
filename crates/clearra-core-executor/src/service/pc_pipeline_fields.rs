mod compact_problem_fields {
    use clearra_core_ffi::CPackingProblem;

    use crate::service::field;

    pub(super) fn compact_problem_fields(problem: &CPackingProblem) -> Vec<(String, String)> {
        vec![
            field("executor_layer", "clearra-core-executor"),
            field("core_problem", "C PackingProblem"),
            field("compact_problem_descriptor", "clr_packing_problem"),
            field("compact_problem_kind", problem.problem_kind),
            field("compact_board_width", problem.board.width),
            field(
                "compact_piece_source_kind",
                problem.piece_source.source_kind,
            ),
            field(
                "compact_piece_source_id",
                problem.piece_source.piece_source_id,
            ),
            field(
                "compact_piece_multiset_count",
                problem.piece_multiset_window.total_count,
            ),
            field(
                "compact_supply_provenance_id",
                problem.piece_source.provenance_id,
            ),
            field("compact_rule_profile_id", problem.rule.rule_profile_id),
            field("compact_kick_profile_id", problem.rule.kick_profile_id),
            field(
                "compact_has_verified_kick_profile",
                problem.rule.has_verified_kick_profile,
            ),
            field(
                "compact_verified_kick_transition_count",
                problem.rule.verified_transition_count,
            ),
            field("compact_backend_request", problem.backend.requested_backend),
        ]
    }
}
mod execution_fields {
    use crate::{buildup::BuildUpRunResult, packing::PackingRunResult, service::field};

    pub(super) fn execution_fields(
        packing: &PackingRunResult,
        buildup: &BuildUpRunResult,
    ) -> Vec<(String, String)> {
        vec![
            field("packing_runner", "PackingRunner::run"),
            field("buildup_runner", "BuildUpRunner::run"),
            field(
                "packing_execution_source",
                packing.execution_source().as_str(),
            ),
            field("buildup_execution_source", buildup.execution_source()),
            field(
                "native_c_core_linked",
                clearra_core_ffi::CoreCNative::linked(),
            ),
            field("native_c_core_executed", true),
            field(
                "native_c_core_fallback_policy",
                "native-required-no-fallback",
            ),
            field(
                "packing_algorithm",
                "geometry_catalog_exact_cover_buildable_stream",
            ),
        ]
    }
}
mod gpu_fields {
    use crate::{packing::PackingRunResult, service::field};

    pub(super) fn gpu_fields(packing: &PackingRunResult) -> Vec<(String, String)> {
        let report = packing.gpu_packing_report();
        vec![
            field("gpu_backend_scope", report.backend_scope()),
            field("gpu_backend_available", report.available()),
            field(
                "gpu_packing_backend",
                if report.available() {
                    "native-gpu"
                } else {
                    "unavailable"
                },
            ),
            field(
                "gpu_packing_unavailable_reason",
                report.unavailable_reason(),
            ),
            field(
                "gpu_packing_hash_exact_confirm_required",
                report.hash_exact_confirm_required(),
            ),
            field("gpu_larger_batch_planner", report.larger_batch_planner()),
            field("gpu_dominance_prefilter", report.dominance_prefilter()),
            field("gpu_shape_union_mask", report.shape_union_mask()),
            field("gpu_candidate_hash", report.candidate_hash()),
            field("gpu_readback_compression", report.readback_compression()),
            field(
                "gpu_cpu_exact_confirm_optimized",
                report.cpu_exact_confirm_optimized(),
            ),
            field("gpu_result_deterministic", report.deterministic_result()),
            field("gpu_result_cpu_confirmed", report.cpu_reference_confirmed()),
            field("gpu_cpu_reference_match", report.cpu_reference_match()),
        ]
    }
}
mod hybrid_fields {
    use crate::{packing::PackingRunResult, service::field};

    pub(super) fn hybrid_fields(packing: &PackingRunResult) -> Vec<(String, String)> {
        let report = packing.hybrid_scheduler_report();
        let memory = packing.memory_report();
        vec![
            field(
                "gpu_assisted_buildup_reached",
                report.gpu_assisted_buildup_reached(),
            ),
            field(
                "gpu_only_packing_cpu_buildup_matches_cpu_reference",
                report.gpu_only_packing_cpu_buildup_matches_cpu_reference(),
            ),
            field(
                "gpu_cpu_reference_candidate_count",
                report.cpu_reference_candidate_count(),
            ),
            field(
                "gpu_hybrid_candidate_count",
                report.hybrid_candidate_count(),
            ),
            field(
                "gpu_cpu_reference_build_variant_count",
                report.cpu_reference_build_variant_count(),
            ),
            field(
                "gpu_hybrid_build_variant_count",
                report.hybrid_build_variant_count(),
            ),
            field(
                "gpu_cpu_reference_coverage_row_count",
                report.cpu_reference_coverage_row_count(),
            ),
            field(
                "gpu_hybrid_coverage_row_count",
                report.hybrid_coverage_row_count(),
            ),
            field(
                "gpu_coverage_rows_from_enumerate_variants",
                report.coverage_rows_from_enumerate_variants(),
            ),
            field(
                "gpu_verify_first_used_for_coverage",
                report.verify_first_used_for_coverage(),
            ),
            field("hybrid_scheduler", report.enabled()),
            field(
                "hybrid_gpu_large_packing_batch",
                report.gpu_large_packing_batch(),
            ),
            field(
                "hybrid_cpu_small_irregular_buildup",
                report.cpu_small_irregular_buildup(),
            ),
            field(
                "hybrid_gpu_readback_cpu_buildup_overlap",
                report.gpu_readback_cpu_buildup_overlap(),
            ),
            field("hybrid_batch_buffer_reuse", report.batch_buffer_reuse()),
            field("hybrid_memory_epoch_managed", report.memory_epoch_managed()),
            field(
                "hybrid_backend_metrics_reported",
                report.backend_metrics_reported(),
            ),
            field(
                "hybrid_candidate_queue_len",
                report.hybrid_candidate_queue_len(),
            ),
            field(
                "hybrid_candidate_queue_capacity",
                report.hybrid_candidate_queue_capacity(),
            ),
            field(
                "hybrid_cpu_worker_backlog",
                report.hybrid_cpu_worker_backlog(),
            ),
            field(
                "hybrid_gpu_readback_backlog",
                report.hybrid_gpu_readback_backlog(),
            ),
            field(
                "hybrid_gpu_batch_in_flight",
                report.hybrid_gpu_batch_in_flight(),
            ),
            field(
                "hybrid_backpressure_active",
                report.hybrid_backpressure_active(),
            ),
            field(
                "hybrid_deferred_batch_count",
                report.hybrid_deferred_batch_count(),
            ),
            field(
                "hybrid_truncated_batch_count",
                report.hybrid_truncated_batch_count(),
            ),
            field(
                "hybrid_memory_pressure_level",
                report.hybrid_memory_pressure_level(),
            ),
            field("hybrid_fallback_reason", report.fallback_reason()),
            field(
                "hybrid_memory_leak_report_clean",
                memory.memory_leak_report_clean(),
            ),
        ]
    }
}
mod memory_fields {
    use crate::{buildup::BuildUpRunResult, packing::PackingRunResult, service::field};

    pub(super) fn memory_fields(
        packing: &PackingRunResult,
        buildup: &BuildUpRunResult,
    ) -> Vec<(String, String)> {
        let report = packing.memory_report();
        let buildup_workspace_bytes = buildup.peak_workspace_bytes();
        let retained_search_bytes = report
            .retained_search_bytes()
            .saturating_add(buildup_workspace_bytes);
        let peak_cpu_bytes = report
            .peak_cpu_bytes()
            .saturating_add(buildup_workspace_bytes);
        vec![
            field(
                "memory_leak_report_clean",
                report.memory_leak_report_clean(),
            ),
            field(
                "memory_leak_check_state",
                report.leak_check_state().as_str(),
            ),
            field(
                "memory_transient_scope_release_complete",
                report.transient_scope_release_complete(),
            ),
            field("live_scopes", 0),
            field("live_allocations", report.transient_live_allocations()),
            field(
                "retained_candidate_allocations",
                report.retained_candidate_allocation_count(),
            ),
            field(
                "retained_candidate_bytes",
                report.retained_candidate_bytes(),
            ),
            field(
                "retained_pattern_index_allocations",
                report.retained_pattern_index_allocation_count(),
            ),
            field(
                "retained_pattern_index_bytes",
                report.retained_pattern_index_bytes(),
            ),
            field(
                "retained_search_allocations",
                report.retained_allocation_count(),
            ),
            field("retained_search_bytes", retained_search_bytes),
            field("buildup_workspace_bytes", buildup_workspace_bytes),
            field("peak_cpu_bytes", peak_cpu_bytes),
            field("live_gpu_buffers", 0),
            field("pending_release_queue", 0),
            field("pending_gpu_buffer_releases", 0),
            field("double_releases", 0),
            field("canary_failures", 0),
            field("poison_detections", 0),
            field("memory_pressure_level", report.pressure_level()),
        ]
    }
}
mod objective_fields {
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;

    use crate::{buildup::BuildUpRunResult, service::field};

    pub(super) fn objective_fields(
        buildup: &BuildUpRunResult,
        line_label: u8,
        objective_policy: ObjectivePolicy,
    ) -> Vec<(String, String)> {
        let objective = buildup.objective_result();
        let mut fields = vec![
            field("objective_search_complete", buildup.objective_complete()),
            field(
                "objective_search_incomplete_reason",
                buildup.objective_incomplete_reason().unwrap_or("none"),
            ),
            field("objective_complete", buildup.objective_complete()),
            field(
                "objective_incomplete_reason",
                buildup.objective_incomplete_reason().unwrap_or("none"),
            ),
            field(
                "objective_coverage_matrix_rows",
                buildup.coverage_row_count(),
            ),
            field(
                "objective_min_cover_selected_rows",
                objective
                    .map(|result| result.minimum_cover().row_indices().len())
                    .unwrap_or(0),
            ),
            field("cleared_line_label", line_label),
        ];
        if objective_policy.score().requested() {
            fields.extend([
                field("score_matrix_materialized", false),
                field("score_matrix_complete", false),
                field(
                    "score_matrix_incomplete_reason",
                    "score_matrix_not_materialized",
                ),
            ]);
        }
        fields
    }
}
mod pipeline_fields {
    use clearra_core_ffi::CPackingProblem;

    use crate::{
        buildup::BuildUpRunResult,
        packing::PackingRunResult,
        service::pc_pipeline_fields::{
            compact_problem_fields::compact_problem_fields, execution_fields::execution_fields,
            gpu_fields::gpu_fields, hybrid_fields::hybrid_fields, memory_fields::memory_fields,
            objective_fields::objective_fields, result_contract_fields::result_contract_fields,
        },
    };

    pub(crate) fn core_pipeline_fields(
        compact_problem: &CPackingProblem,
        packing: &PackingRunResult,
        buildup: &BuildUpRunResult,
        line_label: u8,
        objective_policy: clearra_objectives::policy::objective_policy::ObjectivePolicy,
    ) -> Vec<(String, String)> {
        let mut fields = compact_problem_fields(compact_problem);
        fields.extend(execution_fields(packing, buildup));
        fields.extend(gpu_fields(packing));
        fields.extend(hybrid_fields(packing));
        fields.extend(memory_fields(packing, buildup));
        fields.extend(result_contract_fields(packing, buildup, objective_policy));
        fields.extend(objective_fields(buildup, line_label, objective_policy));
        fields
    }
}
mod result_contract_fields {
    use clearra_objectives::policy::objective_policy::ObjectivePolicy;

    use crate::{buildup::BuildUpRunResult, packing::PackingRunResult, service::field};

    pub(super) fn result_contract_fields(
        packing: &PackingRunResult,
        buildup: &BuildUpRunResult,
        objective_policy: ObjectivePolicy,
    ) -> Vec<(String, String)> {
        vec![
            field("buildup", "core-c-buildup-runner"),
            field("coverage_row_source", "c-coverage-row-view"),
            field("search_execution_report", "attached"),
            field("backend_report", "attached"),
            field("packing_result", "C PackingResult"),
            field("packing_candidate_view", "C PackingCandidateView"),
            field("c_packing_result", "attached"),
            field("buildup_result", "C BuildUpResult"),
            field("c_buildup_result", "attached"),
            field(
                "build_variant_view",
                if buildup.solution_found() {
                    "attached"
                } else {
                    "none"
                },
            ),
            field("execution_variant_set", "attached"),
            field("coverage_row_view", "CCoverageRowView"),
            field("coverage_result", "rust-coverage"),
            field("coverage_reducer", "pattern-bitset-union"),
            field("coverage_source", buildup.coverage_source()),
            field("coverage_rows", buildup.coverage_row_count()),
            field("objective_result", "rust-objective-reducer"),
            field("rust_objective_reducer", "ObjectiveReducer::reduce"),
            field("rust_output_model", "CoreExecutionResult"),
            field("replay_trace", "clearra-replay-sample-contract"),
            field(
                "postprocess_scoring_requested",
                objective_policy.score().requested(),
            ),
            field(
                "score_objective_mode",
                objective_policy.score().mode().as_str(),
            ),
            field(
                "score_profile_requested",
                objective_policy.score().profile().as_str(),
            ),
            field(
                "spin_profile_requested",
                objective_policy.score().spin_profile().as_str(),
            ),
            field("score_initial_b2b", objective_policy.score().initial_b2b()),
            field(
                "postprocess_execution_owner",
                "clearra-app->clearra-postprocess",
            ),
            field(
                "postprocess_replay_seed_available",
                buildup.sample_replay_trace().is_some(),
            ),
            field(
                "postprocess_execution_count",
                buildup.postprocess_executions().len(),
            ),
            field(
                "postprocess_execution_complete",
                buildup.postprocess_execution_complete(),
            ),
            field(
                "postprocess_pattern_weight_count",
                buildup.postprocess_pattern_weights().len(),
            ),
            field("packing_candidate_is_solution", "false"),
            field("packing_candidate_count", packing.candidate_count()),
            field(
                "packing_multiset_group_count",
                packing.multiset_group_count(),
            ),
            field(
                "packing_pattern_membership_kind",
                packing.multiset_membership_kind().as_str(),
            ),
            field("packing_count_complete", packing.count_complete()),
            field(
                "packing_truncation_reason",
                packing
                    .truncation_reason()
                    .map(|reason| reason.as_str())
                    .unwrap_or("none"),
            ),
            field("build_variant_count", buildup.build_variant_count()),
            field(
                "pattern_verified_execution_count",
                buildup.pattern_verified_execution_count(),
            ),
            field("unique_trace_count", buildup.unique_trace_count()),
            field("coverage_row_count", buildup.coverage_row_count()),
            field("coverage_probability", buildup.coverage_probability()),
        ]
    }
}

pub(crate) use pipeline_fields::core_pipeline_fields;
