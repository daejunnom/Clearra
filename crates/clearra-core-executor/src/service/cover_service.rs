mod coverage_fields {
    use clearra_build_coverage::coverage::BuildCoverageExecution;

    use crate::{
        buildup::BuildUpRunResult,
        packing::PackingRunResult,
        service::{cover_service::probability_formatter::format_probability, field},
    };

    pub(super) fn coverage_fields(
        packing: &PackingRunResult,
        buildup: &BuildUpRunResult,
        coverage: &BuildCoverageExecution,
    ) -> Vec<(String, String)> {
        vec![
            field("assignment_solver", "AssignmentExactCoverBridge"),
            field("assignment_csp", "AssignmentCsp"),
            field("assignment_count", coverage.assignments().len()),
            field(
                "assignment_exact_cover_complete",
                coverage.exact_cover_complete(),
            ),
            field(
                "assignment_exact_cover_searched_nodes",
                coverage.exact_cover_searched_nodes(),
            ),
            field("packing_candidate_count", packing.candidate_count()),
            field("core_buildup_variant_count", buildup.build_variant_count()),
            field(
                "pattern_verified_execution_count",
                buildup.pattern_verified_execution_count(),
            ),
            field("unique_trace_count", buildup.unique_trace_count()),
            field("core_coverage_row_count", buildup.coverage_row_count()),
            field("c_coverage_row_count", coverage.c_coverage_row_count()),
            field(
                "c_buildup_coverage_row_generated",
                coverage.c_coverage_row_count() > 0,
            ),
            field("coverage_row_identity_validated", "true"),
            field("slot_domain_policy", "bridge-repeated-standard-pieces"),
            field(
                "coverage_matrix_row_count",
                coverage.matrix().matrix().rows().len(),
            ),
            field(
                "union_covered_patterns",
                coverage.union().covered_patterns().count_ones(),
            ),
            field(
                "build_coverage_probability",
                format_probability(coverage.result().probability().get()),
            ),
            field("coverage_row_source", "C BuildUp coverage row"),
            field("coverage_reducer", "pattern-bitset-union"),
            field("rust_objective_reducer", "ObjectiveReducer::reduce"),
            field(
                "union_probability_reducer",
                "BuildCoverageResult uses union probability",
            ),
            field("cover_reports_union_probability", "true"),
            field("cover_reports_c_coverage_row_count", "true"),
            field("slot_assignment_count_is_not_success_probability", "true"),
            field("success_probability_source", "UnionProbability"),
            field("rust_output_model", "CoreExecutionResult"),
        ]
    }
}
mod error {
    use clearra_build_coverage::coverage::BuildCoverageExecutionError;

    use crate::{buildup::BuildUpRunnerError, packing::PackingRunnerError};

    #[derive(Clone, Debug, PartialEq)]
    pub enum CoverServiceError {
        UnsupportedPreset,
        Packing(PackingRunnerError),
        BuildUp(BuildUpRunnerError),
        Coverage(BuildCoverageExecutionError),
    }
}
mod gpu_fields {
    use crate::{packing::PackingRunResult, service::field};

    pub(super) fn gpu_fields(packing: &PackingRunResult) -> Vec<(String, String)> {
        let report = packing.gpu_packing_report();
        vec![
            field("gpu_backend_scope", report.backend_scope()),
            field("gpu_backend_available", "false"),
            field("gpu_packing_backend", "unavailable"),
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
        vec![
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
                packing.memory_report().memory_leak_report_clean(),
            ),
        ]
    }
}
mod pipeline_fields {
    use clearra_problem::SearchProblem;

    use crate::{packing::PackingRunResult, service::field};

    pub(super) fn pipeline_fields(
        problem: &SearchProblem,
        packing: &PackingRunResult,
    ) -> Vec<(String, String)> {
        let compact_problem = packing.compact_problem();
        vec![
            field("status", "cover-executed"),
            field("execution_scope", "m21-build-coverage-product-path"),
            field("product_scope", "M21 Build Coverage Product Path"),
            field(
                "executor_flow",
                "SearchProblem->C PackingProblem->C PackingResult->C BuildUpResult->CoverageRows->Rust ObjectiveResult->Rust OutputModel",
            ),
            field(
                "build_coverage_flow",
                "BuildTemplate -> SlotDomain -> SlotAssignment -> BuildUpProblem -> C BuildUp -> CoverageRow -> CoverageMatrix -> UnionProbability",
            ),
            field("problem_layer", "clearra-problem"),
            field("problem_preset", "build"),
            field("problem_source", problem.scenario().source().as_str()),
            field("compiled_goal", problem.goal().as_str()),
            field("compiled_piece_window", problem.piece_window().max_pieces()),
            field("executor_layer", "clearra-core-executor"),
            field(
                "compact_piece_source_kind",
                compact_problem.piece_source.source_kind,
            ),
            field(
                "compact_piece_source_id",
                compact_problem.piece_source.piece_source_id,
            ),
            field(
                "compact_piece_multiset_count",
                compact_problem.piece_multiset_window.total_count,
            ),
            field(
                "compact_supply_provenance_id",
                compact_problem.piece_source.provenance_id,
            ),
            field("packing_runner", "PackingRunner::run"),
            field("buildup_runner", "BuildUpRunner::run"),
            field("route", "search-problem-core-executor"),
        ]
    }
}
mod probability_formatter {
    pub(super) fn format_probability(value: f64) -> String {
        if value == 0.0 {
            return "0.0".to_owned();
        }
        if value == 1.0 {
            return "1.0".to_owned();
        }

        format!("{value:.12}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    }
}
mod query_bridge {
    use clearra_build_coverage::{
        domain::slot_domain::SlotDomain,
        query::{
            build_coverage_limits::BuildCoverageLimits, build_coverage_query::BuildCoverageQuery,
        },
        template::{BuildSlot, BuildSlotId, BuildTemplate},
    };
    use clearra_core_domain::{board::cell::CellCoord, piece::piece_kind::PieceKind};
    use clearra_problem::BuildQuery;

    pub(super) fn build_coverage_query_from_bridge(query: &BuildQuery) -> BuildCoverageQuery {
        let board_size = query.template().board_size();
        let width = usize::from(board_size.width()).max(1);
        let height = usize::from(board_size.height()).max(1);
        let slots = (0..query.template().slot_count())
            .map(|index| {
                let slot_id = BuildSlotId::new(index as u32);
                let x = (index % width) as u16;
                let y = ((index / width) % height) as u16;
                let cell =
                    CellCoord::new(x, y, board_size).unwrap_or(CellCoord::new_unchecked(x, y));
                BuildSlot::new(slot_id, vec![cell])
            })
            .collect::<Vec<_>>();
        let domains = slots
            .iter()
            .enumerate()
            .map(|(index, slot)| {
                let piece =
                    PieceKind::STANDARD_TETROMINOES[index % PieceKind::STANDARD_TETROMINOES.len()];
                SlotDomain::new(slot.id(), vec![piece])
            })
            .collect::<Vec<_>>();
        let template = BuildTemplate::new(query.template().id(), slots)
            .with_board_size(board_size)
            .with_label(query.template().label().unwrap_or("bridge-template"));

        BuildCoverageQuery::new(
            template,
            domains,
            Vec::new(),
            query.pattern_count(),
            BuildCoverageLimits::new(
                query.limits().max_assignments(),
                query.limits().max_patterns(),
            ),
        )
    }
}
mod resource_fields {
    use crate::{buildup::BuildUpRunResult, packing::PackingRunResult, service::field};

    pub(super) fn resource_fields(
        packing: &PackingRunResult,
        buildup: &BuildUpRunResult,
    ) -> Vec<(String, String)> {
        let report = packing.resource_report();
        let probability_complete = buildup.count_complete();
        let peak_cpu_bytes = report
            .peak_cpu_bytes
            .saturating_add(buildup.peak_workspace_bytes());
        vec![
            field("probability_complete", probability_complete),
            field("count_complete", buildup.count_complete()),
            field("count_truncated_reason", buildup.count_truncated_reason()),
            field("resource_truncated", !probability_complete),
            field(
                "resource_truncation_reason",
                if probability_complete {
                    "none"
                } else {
                    buildup.count_truncated_reason()
                },
            ),
            field("resource_peak_frontier_states", report.peak_frontier_states),
            field("resource_peak_candidate_rows", report.peak_candidate_rows),
            field("resource_peak_hash_buckets", report.peak_hash_buckets),
            field("resource_peak_gpu_bytes", report.peak_gpu_bytes),
            field("resource_peak_cpu_bytes", peak_cpu_bytes),
            field(
                "resource_buildup_workspace_bytes",
                buildup.peak_workspace_bytes(),
            ),
            field(
                "resource_build_worker_backlog_peak",
                report.build_worker_backlog_peak,
            ),
            field(
                "resource_coverage_rows_emitted",
                report
                    .coverage_rows_emitted
                    .max(buildup.coverage_row_count()),
            ),
            field("resource_probability_complete", probability_complete),
        ]
    }
}
mod service {
    use clearra_build_coverage::{
        coverage::BuildCoverageExecution, query::build_coverage_query::BuildCoverageQuery,
    };
    use clearra_core_domain::execution_cancellation::{
        ExecutionCancellationToken, ExecutionControl,
    };
    use clearra_problem::{SearchProblem, SearchProblemPreset};

    use crate::{
        buildup::BuildUpRunner,
        core_execution_result::CoreExecutionResult,
        packing::PackingRunner,
        service::cover_service::{
            coverage_fields::coverage_fields, error::CoverServiceError, gpu_fields::gpu_fields,
            hybrid_fields::hybrid_fields, pipeline_fields::pipeline_fields,
            query_bridge::build_coverage_query_from_bridge, resource_fields::resource_fields,
            template_fields::template_fields,
        },
    };

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct CoverService;

    impl CoverService {
        pub fn execute(problem: &SearchProblem) -> Result<CoreExecutionResult, CoverServiceError> {
            Self::execute_with_cancellation(problem, &ExecutionCancellationToken::new())
        }

        pub fn execute_with_cancellation(
            problem: &SearchProblem,
            cancellation: &ExecutionCancellationToken,
        ) -> Result<CoreExecutionResult, CoverServiceError> {
            Self::execute_with_control(problem, &ExecutionControl::new(cancellation.clone()))
        }

        pub fn execute_with_control(
            problem: &SearchProblem,
            control: &ExecutionControl,
        ) -> Result<CoreExecutionResult, CoverServiceError> {
            Self::require_build_preset(problem)?;
            let query = problem
                .build_query()
                .ok_or(CoverServiceError::UnsupportedPreset)?;
            Self::execute_build_coverage_with_control(
                problem,
                &build_coverage_query_from_bridge(query),
                control,
            )
        }
    }
    impl CoverService {
        pub fn execute_build_coverage(
            problem: &SearchProblem,
            query: &BuildCoverageQuery,
        ) -> Result<CoreExecutionResult, CoverServiceError> {
            Self::execute_build_coverage_with_cancellation(
                problem,
                query,
                &ExecutionCancellationToken::new(),
            )
        }

        pub fn execute_build_coverage_with_cancellation(
            problem: &SearchProblem,
            query: &BuildCoverageQuery,
            cancellation: &ExecutionCancellationToken,
        ) -> Result<CoreExecutionResult, CoverServiceError> {
            Self::execute_build_coverage_with_control(
                problem,
                query,
                &ExecutionControl::new(cancellation.clone()),
            )
        }

        pub fn execute_build_coverage_with_control(
            problem: &SearchProblem,
            query: &BuildCoverageQuery,
            control: &ExecutionControl,
        ) -> Result<CoreExecutionResult, CoverServiceError> {
            Self::require_build_preset(problem)?;
            let build_query = problem
                .build_query()
                .ok_or(CoverServiceError::UnsupportedPreset)?;
            let packing = PackingRunner::run_with_control(problem, control)
                .map_err(CoverServiceError::Packing)?;
            let buildup = BuildUpRunner::run_with_control(problem, &packing, control)
                .map_err(CoverServiceError::BuildUp)?;
            let coverage =
                BuildCoverageExecution::from_c_buildup_rows(query, buildup.coverage_rows())
                    .map_err(CoverServiceError::Coverage)?;

            let mut fields = pipeline_fields(problem, &packing);
            fields.extend(gpu_fields(&packing));
            fields.extend(hybrid_fields(&packing));
            fields.extend(coverage_fields(&packing, &buildup, &coverage));
            fields.extend(resource_fields(&packing, &buildup));
            fields.extend(template_fields(build_query, query));
            Ok(CoreExecutionResult::new(fields, Vec::new()))
        }
    }
    impl CoverService {
        fn require_build_preset(problem: &SearchProblem) -> Result<(), CoverServiceError> {
            if problem.preset() == SearchProblemPreset::Build {
                Ok(())
            } else {
                Err(CoverServiceError::UnsupportedPreset)
            }
        }
    }
}
mod template_fields {
    use clearra_build_coverage::query::build_coverage_query::BuildCoverageQuery;
    use clearra_problem::BuildQuery;

    use crate::service::field;

    pub(super) fn template_fields(
        build_query: &BuildQuery,
        coverage_query: &BuildCoverageQuery,
    ) -> Vec<(String, String)> {
        vec![
            field("template", build_query.template().id()),
            field(
                "template_label",
                build_query.template().label().unwrap_or("none"),
            ),
            field("board_width", build_query.template().board_size().width()),
            field("board_height", build_query.template().board_size().height()),
            field("slot_count", build_query.template().slot_count()),
            field("pattern_count", coverage_query.pattern_count()),
            field("max_assignments", coverage_query.limits().max_assignments()),
            field("max_patterns", coverage_query.limits().max_patterns()),
        ]
    }
}

pub use error::CoverServiceError;
pub use service::CoverService;

#[cfg(all(test, feature = "native-c-core"))]
use clearra_problem::BuildQuery;
#[cfg(all(test, feature = "native-c-core"))]
use query_bridge::build_coverage_query_from_bridge;

#[cfg(all(test, feature = "native-c-core"))]
#[path = "cover_service_tests.rs"]
mod tests;
