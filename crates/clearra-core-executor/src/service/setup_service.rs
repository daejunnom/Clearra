mod context {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_problem::{query::SetupSearchQuery, SearchProblem};

    use crate::{buildup::BuildUpRunResult, packing::PackingRunResult};

    pub(super) struct SetupExecutionContext<'a> {
        pub(super) problem: &'a SearchProblem,
        pub(super) query: &'a SetupSearchQuery,
        pub(super) packing: &'a PackingRunResult,
        pub(super) buildup: &'a BuildUpRunResult,
        pub(super) coverage_pattern_count: usize,
        pub(super) verified_pattern_count: usize,
        pub(super) covered_pattern_count_basis: &'static str,
        pub(super) probability_complete: bool,
        pub(super) count_truncated_reason: &'static str,
        pub(super) queue_prefix: Vec<PieceKind>,
        pub(super) bag_boundary_offsets: Vec<usize>,
    }
}
mod error {
    use crate::{buildup::BuildUpRunnerError, packing::PackingRunnerError};

    #[derive(Clone, Debug, PartialEq)]
    pub enum SetupServiceError {
        UnsupportedPreset,
        Packing(PackingRunnerError),
        BuildUp(BuildUpRunnerError),
    }
}
mod formatters {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    pub(super) fn format_piece_sequence(pieces: &[PieceKind]) -> String {
        if pieces.is_empty() {
            return "none".to_owned();
        }
        pieces.iter().map(|piece| piece.as_ascii()).collect()
    }

    pub(super) fn format_usize_list(values: &[usize]) -> String {
        if values.is_empty() {
            return "none".to_owned();
        }
        values
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",")
    }
}
mod gpu_fields {
    use crate::service::field;

    use super::context::SetupExecutionContext;

    pub(super) fn gpu_fields(context: &SetupExecutionContext<'_>) -> Vec<(String, String)> {
        let gpu = context.packing.gpu_packing_report();
        vec![
            field("gpu_backend_scope", gpu.backend_scope()),
            field("gpu_backend_available", false),
            field("gpu_packing_backend", "unavailable"),
            field("gpu_packing_unavailable_reason", gpu.unavailable_reason()),
            field(
                "gpu_packing_hash_exact_confirm_required",
                gpu.hash_exact_confirm_required(),
            ),
            field("gpu_larger_batch_planner", gpu.larger_batch_planner()),
            field("gpu_dominance_prefilter", gpu.dominance_prefilter()),
            field("gpu_shape_union_mask", gpu.shape_union_mask()),
            field("gpu_candidate_hash", gpu.candidate_hash()),
            field("gpu_readback_compression", gpu.readback_compression()),
            field(
                "gpu_cpu_exact_confirm_optimized",
                gpu.cpu_exact_confirm_optimized(),
            ),
            field("gpu_result_deterministic", gpu.deterministic_result()),
            field("gpu_result_cpu_confirmed", gpu.cpu_reference_confirmed()),
            field("gpu_cpu_reference_match", gpu.cpu_reference_match()),
        ]
    }
}
mod hybrid_fields {
    use crate::service::field;

    use super::context::SetupExecutionContext;

    pub(super) fn hybrid_fields(context: &SetupExecutionContext<'_>) -> Vec<(String, String)> {
        let hybrid = context.packing.hybrid_scheduler_report();
        vec![
            field("hybrid_scheduler", hybrid.enabled()),
            field(
                "hybrid_gpu_large_packing_batch",
                hybrid.gpu_large_packing_batch(),
            ),
            field(
                "hybrid_cpu_small_irregular_buildup",
                hybrid.cpu_small_irregular_buildup(),
            ),
            field(
                "hybrid_gpu_readback_cpu_buildup_overlap",
                hybrid.gpu_readback_cpu_buildup_overlap(),
            ),
            field("hybrid_batch_buffer_reuse", hybrid.batch_buffer_reuse()),
            field("hybrid_memory_epoch_managed", hybrid.memory_epoch_managed()),
            field(
                "hybrid_backend_metrics_reported",
                hybrid.backend_metrics_reported(),
            ),
            field(
                "hybrid_candidate_queue_len",
                hybrid.hybrid_candidate_queue_len(),
            ),
            field(
                "hybrid_candidate_queue_capacity",
                hybrid.hybrid_candidate_queue_capacity(),
            ),
            field(
                "hybrid_cpu_worker_backlog",
                hybrid.hybrid_cpu_worker_backlog(),
            ),
            field(
                "hybrid_gpu_readback_backlog",
                hybrid.hybrid_gpu_readback_backlog(),
            ),
            field(
                "hybrid_gpu_batch_in_flight",
                hybrid.hybrid_gpu_batch_in_flight(),
            ),
            field(
                "hybrid_backpressure_active",
                hybrid.hybrid_backpressure_active(),
            ),
            field(
                "hybrid_deferred_batch_count",
                hybrid.hybrid_deferred_batch_count(),
            ),
            field(
                "hybrid_truncated_batch_count",
                hybrid.hybrid_truncated_batch_count(),
            ),
            field(
                "hybrid_memory_pressure_level",
                hybrid.hybrid_memory_pressure_level(),
            ),
            field("hybrid_fallback_reason", hybrid.fallback_reason()),
            field(
                "hybrid_memory_leak_report_clean",
                context.packing.memory_report().memory_leak_report_clean(),
            ),
        ]
    }
}
mod pipeline_fields {
    use crate::service::field;

    use super::context::SetupExecutionContext;

    pub(super) fn pipeline_fields(context: &SetupExecutionContext<'_>) -> Vec<(String, String)> {
        let compact = context.packing.compact_problem();
        vec![
            field("status", "setup-executed"),
            field("execution_scope", "m20-setup-search-product-path"),
            field("executor_flow", "SearchProblem->C PackingProblem->C PackingResult->C BuildUpResult->CoverageRows->Rust ObjectiveResult->Rust OutputModel"),
            field("problem_layer", "clearra-problem"),
            field("problem_preset", "setup"),
            field("problem_source", context.problem.scenario().source().as_str()),
            field("compiled_goal", context.problem.goal().as_str()),
            field("compiled_piece_window", context.problem.piece_window().max_pieces()),
            field("compiled_initial_board_mask", format!("0x{:016x}", context.problem.initial_board().occupied_mask())),
            field("executor_layer", "clearra-core-executor"),
            field("compact_piece_source_kind", compact.piece_source.source_kind),
            field("compact_piece_source_id", compact.piece_source.piece_source_id),
            field("compact_piece_multiset_count", compact.piece_multiset_window.total_count),
            field("compact_supply_provenance_id", compact.piece_source.provenance_id),
            field("packing_runner", "PackingRunner::run"),
        ]
    }
}
mod query_fields {
    use crate::service::field;

    use super::context::SetupExecutionContext;

    pub(super) fn query_fields(context: &SetupExecutionContext<'_>) -> Vec<(String, String)> {
        vec![
            field("queue_mode", context.query.queue().mode()),
            field("queue_len", context.query.queue().len()),
            field("board_width", context.query.board_size().width()),
            field("board_height", context.query.board_size().height()),
            field("lines", context.query.target().lines()),
            field("piece_window", context.problem.piece_window().max_pieces()),
            field("max_results", context.query.limits().max_results()),
            field("max_patterns", context.query.limits().max_patterns()),
            field("score_aggregation_attached", false),
            field("route", "search-problem-core-executor"),
        ]
    }
}
mod raw_metrics_fields {
    use crate::service::field;

    use super::context::SetupExecutionContext;

    pub(super) fn raw_metrics_fields(context: &SetupExecutionContext<'_>) -> Vec<(String, String)> {
        vec![
            field(
                "raw_coverage_export_path",
                "inline://clearra/setup/raw-coverage/setup-family-0/union",
            ),
            field("setup_raw_metrics", "attached"),
            field("setup_raw_coverage_export", "inline"),
            field("raw_coverage_schema_version", 2),
            field("raw_coverage_export_kind", "setup_raw_coverage_export"),
            field(
                "pattern_universe_id",
                context
                    .problem
                    .piece_source()
                    .pattern_universe_id()
                    .map_or(0, |id| id.get()),
            ),
            field(
                "pattern_weight_model_id",
                context
                    .problem
                    .piece_source()
                    .pattern_weight_model_id()
                    .map_or(0, |id| id.get()),
            ),
            field("pattern_count", context.coverage_pattern_count),
            field("rows", "machine-readable-coverage-rows"),
            field("family_unions", "machine-readable-family-unions"),
            field("overlap_report", "visible"),
            field(
                "coverage_overlap_report",
                "union-probability-no-variant-sum",
            ),
            field("build_variant_metrics", "per-result"),
            field("diagnostic_evidence", "attached"),
            field("coverage_reducer", "pattern-bitset-union"),
        ]
    }
}
mod resource_fields {
    use crate::service::field;

    use super::context::SetupExecutionContext;

    pub(super) fn resource_fields(context: &SetupExecutionContext<'_>) -> Vec<(String, String)> {
        let resource = context.packing.resource_report();
        let peak_cpu_bytes = resource
            .peak_cpu_bytes
            .saturating_add(context.buildup.peak_workspace_bytes());
        vec![
            field("resource_truncated", !context.probability_complete),
            field(
                "resource_truncation_reason",
                if context.probability_complete {
                    "none"
                } else {
                    context.count_truncated_reason
                },
            ),
            field(
                "resource_peak_frontier_states",
                resource.peak_frontier_states,
            ),
            field("resource_peak_candidate_rows", resource.peak_candidate_rows),
            field("resource_peak_hash_buckets", resource.peak_hash_buckets),
            field("resource_peak_gpu_bytes", resource.peak_gpu_bytes),
            field("resource_peak_cpu_bytes", peak_cpu_bytes),
            field(
                "resource_buildup_workspace_bytes",
                context.buildup.peak_workspace_bytes(),
            ),
            field(
                "resource_build_worker_backlog_peak",
                resource.build_worker_backlog_peak,
            ),
            field(
                "resource_coverage_rows_emitted",
                resource
                    .coverage_rows_emitted
                    .max(context.buildup.coverage_row_count()),
            ),
            field(
                "resource_probability_complete",
                context.probability_complete,
            ),
        ]
    }
}
mod service {
    use clearra_core_domain::execution_cancellation::{
        ExecutionCancellationToken, ExecutionControl,
    };
    use clearra_problem::{SearchProblem, SearchProblemPreset};

    use crate::{
        buildup::{
            buildup_coverage_bridge::{
                covered_pattern_count_basis_for_problem, pattern_count as buildup_pattern_count,
                verified_pattern_count_for_execution,
            },
            BuildUpRunner,
        },
        core_execution_result::CoreExecutionResult,
        packing::PackingRunner,
    };

    use super::{
        context::SetupExecutionContext,
        gpu_fields::gpu_fields,
        hybrid_fields::hybrid_fields,
        pipeline_fields::pipeline_fields,
        query_fields::query_fields,
        raw_metrics_fields::raw_metrics_fields,
        resource_fields::resource_fields,
        solution_fields::solution_fields,
        supply_fields::supply_fields,
        supply_materializer::{query_bag_boundary_offsets, query_queue_prefix},
        SetupServiceError,
    };

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct SetupService;

    impl SetupService {
        pub fn execute(problem: &SearchProblem) -> Result<CoreExecutionResult, SetupServiceError> {
            Self::execute_with_cancellation(problem, &ExecutionCancellationToken::new())
        }

        pub fn execute_with_cancellation(
            problem: &SearchProblem,
            cancellation: &ExecutionCancellationToken,
        ) -> Result<CoreExecutionResult, SetupServiceError> {
            Self::execute_with_control(problem, &ExecutionControl::new(cancellation.clone()))
        }

        pub fn execute_with_control(
            problem: &SearchProblem,
            control: &ExecutionControl,
        ) -> Result<CoreExecutionResult, SetupServiceError> {
            if problem.preset() != SearchProblemPreset::Setup {
                return Err(SetupServiceError::UnsupportedPreset);
            }
            let query = problem
                .setup_query()
                .ok_or(SetupServiceError::UnsupportedPreset)?;
            let packing = PackingRunner::run_with_control(problem, control)
                .map_err(SetupServiceError::Packing)?;
            let buildup = BuildUpRunner::run_with_control(problem, &packing, control)
                .map_err(SetupServiceError::BuildUp)?;
            let coverage_pattern_count = buildup_pattern_count(problem);
            let verified_pattern_count = verified_pattern_count_for_execution(
                problem,
                coverage_pattern_count,
                packing.count_complete() && buildup.count_complete(),
            )
            .map_err(SetupServiceError::BuildUp)?;
            let observed_supply_incomplete = !problem.piece_source().complete();
            let probability_complete = !observed_supply_incomplete && buildup.count_complete();
            let context = SetupExecutionContext {
                problem,
                query,
                packing: &packing,
                buildup: &buildup,
                coverage_pattern_count,
                verified_pattern_count,
                covered_pattern_count_basis: covered_pattern_count_basis_for_problem(problem),
                probability_complete,
                count_truncated_reason: if observed_supply_incomplete {
                    "observed_universe_truncated"
                } else {
                    buildup.count_truncated_reason()
                },
                queue_prefix: query_queue_prefix(query),
                bag_boundary_offsets: query_bag_boundary_offsets(query.queue()),
            };

            let mut fields = pipeline_fields(&context);
            fields.extend(gpu_fields(&context));
            fields.extend(hybrid_fields(&context));
            fields.extend(solution_fields(&context));
            fields.extend(resource_fields(&context));
            fields.extend(supply_fields(&context));
            fields.extend(raw_metrics_fields(&context));
            fields.extend(query_fields(&context));
            Ok(CoreExecutionResult::new(fields, Vec::new()))
        }
    }
}
mod solution_fields {
    use crate::service::field;

    use super::context::SetupExecutionContext;

    pub(super) fn solution_fields(context: &SetupExecutionContext<'_>) -> Vec<(String, String)> {
        let buildup = context.buildup;
        vec![
            field("buildup_runner", "BuildUpRunner::run"),
            field("rust_output_model", "CoreExecutionResult"),
            field(
                "enumeration_strategy",
                "shape-family-tiling-build-core-buildup",
            ),
            field("post_pc_evaluation_attached", false),
            field(
                "setup_foundation_reason",
                "core_packing_buildup_build_variants_attached",
            ),
            field("build_variant_source", "C BuildUp"),
            field("packing_candidate_count", context.packing.candidate_count()),
            field("core_buildup_variant_count", buildup.build_variant_count()),
            field("core_coverage_row_count", buildup.covered_pattern_count()),
            field("coverage_source", buildup.coverage_source()),
            field("shape_family_id", "setup-family-0"),
            field("setup_raw_metrics_schema_version", 2),
            field("metrics_kind", "setup_raw_metrics"),
            field("shape_family_count", 1),
            field("tiling_variant_count", context.packing.candidate_count()),
            field("build_variant_count", buildup.build_variant_count()),
            field(
                "pattern_verified_execution_count",
                buildup.pattern_verified_execution_count(),
            ),
            field("unique_trace_count", buildup.unique_trace_count()),
            field("coverage_pattern_count", context.coverage_pattern_count),
            field("verified_pattern_count", context.verified_pattern_count),
            field("materialized_pattern_count", context.coverage_pattern_count),
            field("covered_pattern_count", buildup.covered_pattern_count()),
            field(
                "covered_pattern_count_basis",
                context.covered_pattern_count_basis,
            ),
            field("coverage_probability", buildup.coverage_probability()),
            field("probability_complete", context.probability_complete),
            field("count_complete", context.probability_complete),
            field("count_truncated_reason", context.count_truncated_reason),
        ]
    }
}
mod supply_fields {
    use crate::service::field;

    use super::{
        context::SetupExecutionContext,
        formatters::{format_piece_sequence, format_usize_list},
    };

    pub(super) fn supply_fields(context: &SetupExecutionContext<'_>) -> Vec<(String, String)> {
        let hold_piece = context.query.hold_policy().initial_piece();
        vec![
            field("queue_prefix", format_piece_sequence(&context.queue_prefix)),
            field("queue_prefix_len", context.queue_prefix.len()),
            field("hold_required", hold_piece.is_some()),
            field(
                "hold_piece",
                hold_piece
                    .map(|piece| piece.as_ascii().to_string())
                    .unwrap_or_else(|| "none".to_owned()),
            ),
            field(
                "bag_boundary_offsets",
                format_usize_list(&context.bag_boundary_offsets),
            ),
            field(
                "bag_boundary_ambiguous",
                context.bag_boundary_offsets.len() != 1,
            ),
            field("requires_180", false),
            field("requires_180_evidence", "not-modeled"),
            field(
                "rule_profile_evidence",
                context.problem.rule_profile().rule().id().as_str(),
            ),
            field("post_pc_solution_count", 0),
            field("score_basis", "none"),
            field("backend_report", "attached"),
        ]
    }
}
mod supply_materializer {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_problem::query::{SetupQueueInput, SetupSearchQuery};
    use clearra_supply::bag::bag_boundary::standard_7_bag_observed_boundary_report;

    pub(super) fn query_queue_prefix(query: &SetupSearchQuery) -> Vec<PieceKind> {
        queue_pieces(query.queue())
            .into_iter()
            .take(query.piece_budget().max_piece_count() as usize)
            .collect()
    }

    pub(super) fn query_bag_boundary_offsets(queue: &SetupQueueInput) -> Vec<usize> {
        match queue {
            SetupQueueInput::BagAlignedPattern(pattern) => {
                if pattern.is_empty() {
                    Vec::new()
                } else {
                    vec![0]
                }
            }
            SetupQueueInput::FixedSequence(_) | SetupQueueInput::Observed(_) => {
                standard_7_bag_observed_boundary_report(&queue_pieces(queue))
                    .candidates()
                    .iter()
                    .map(|candidate| candidate.initial_offset())
                    .collect()
            }
        }
    }

    fn queue_pieces(queue: &SetupQueueInput) -> Vec<PieceKind> {
        match queue {
            SetupQueueInput::FixedSequence(sequence) => sequence.pieces().to_vec(),
            SetupQueueInput::BagAlignedPattern(pattern) => pattern.pieces().to_vec(),
            SetupQueueInput::Observed(queue) => queue.pieces().to_vec(),
        }
    }
}

pub use error::SetupServiceError;
pub use service::SetupService;

#[cfg(all(test, feature = "native-c-core"))]
#[path = "setup_service_tests.rs"]
mod tests;
