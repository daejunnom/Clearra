use clearra_core_domain::{
    execution_cancellation::ExecutionCancellationToken,
    resource::{ResourceReport, ResourceTruncationReason},
};
use clearra_core_ffi::NativeCoreError;
use clearra_problem::{SearchProblem, SearchProblemPreset};

use crate::{
    buildup::{
        buildup_coverage_bridge::{
            COVERED_PATTERN_BASIS_COMPLETE_PATTERN_UNIVERSE,
            COVERED_PATTERN_BASIS_MATERIALIZED_PATTERN_UNIVERSE,
            OBSERVED_MATERIALIZED_PATTERN_SPECIFIC,
        },
        BuildUpRunner, BuildUpRunnerError,
    },
    core_execution_result::CoreExecutionResult,
    packing::{PackingRunner, PackingRunnerError},
    service::field,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PercentServiceError {
    UnsupportedPreset,
    EmptyPatternUniverse,
    InvalidCoverageProbability,
    Packing(PackingRunnerError),
    BuildUp(BuildUpRunnerError),
}

impl PercentServiceError {
    pub const fn unsupported_reason(&self) -> Option<&'static str> {
        match self {
            Self::UnsupportedPreset => Some("problem_runtime_unsupported"),
            Self::Packing(PackingRunnerError::Native(NativeCoreError::Unavailable)) => {
                Some("core_c_packing_runtime_unavailable")
            }
            Self::Packing(PackingRunnerError::Backend(error)) => Some(error.reason()),
            Self::BuildUp(BuildUpRunnerError::Native(NativeCoreError::Unavailable)) => {
                Some("core_c_buildup_runtime_unavailable")
            }
            Self::BuildUp(BuildUpRunnerError::UnsupportedPieceSource { reason }) => Some(reason),
            Self::EmptyPatternUniverse
            | Self::InvalidCoverageProbability
            | Self::Packing(_)
            | Self::BuildUp(_) => None,
        }
    }
}

impl PercentServiceError {
    pub const fn resource_incomplete(&self) -> Option<(&'static str, i32, ResourceReport)> {
        match self {
            Self::Packing(PackingRunnerError::Native(NativeCoreError::PackingIncomplete {
                status,
                resource_report,
            })) => Some(("packing", *status, *resource_report)),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PercentService;

impl PercentService {
    pub fn execute(problem: &SearchProblem) -> Result<CoreExecutionResult, PercentServiceError> {
        Self::execute_with_cancellation(problem, &ExecutionCancellationToken::new())
    }

    pub fn execute_with_cancellation(
        problem: &SearchProblem,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<CoreExecutionResult, PercentServiceError> {
        if problem.preset() != SearchProblemPreset::ScenarioPc {
            return Err(PercentServiceError::UnsupportedPreset);
        }
        let universe = problem
            .piece_source()
            .materialized_universe()
            .ok_or(PercentServiceError::EmptyPatternUniverse)?;
        if universe.pattern_count() == 0 {
            return Err(PercentServiceError::EmptyPatternUniverse);
        }

        let packing = PackingRunner::run_with_cancellation(problem, cancellation)
            .map_err(PercentServiceError::Packing)?;
        let buildup = BuildUpRunner::run_with_cancellation(problem, &packing, cancellation)
            .map_err(PercentServiceError::BuildUp)?;
        let probability_complete = problem.piece_source().complete()
            && packing.count_complete()
            && buildup.count_complete();
        let truncation_reason = truncation_reason(problem, &packing, &buildup);
        let mut resource_report = packing.resource_report().clone();
        resource_report.coverage_rows_emitted = buildup.coverage_row_count();
        resource_report.peak_cpu_bytes = resource_report
            .peak_cpu_bytes
            .saturating_add(buildup.peak_workspace_bytes());
        resource_report.probability_complete = probability_complete;
        if !probability_complete && !resource_report.truncated {
            resource_report.mark_truncated(ResourceTruncationReason::ObservedUniverseTruncated);
        }

        build_result(
            problem,
            &packing,
            &buildup,
            resource_report,
            probability_complete,
            truncation_reason,
        )
    }
}

fn build_result(
    problem: &SearchProblem,
    packing: &crate::packing::PackingRunResult,
    buildup: &crate::buildup::BuildUpRunResult,
    resource_report: ResourceReport,
    probability_complete: bool,
    truncation_reason: &'static str,
) -> Result<CoreExecutionResult, PercentServiceError> {
    let source = problem.piece_source();
    let universe = source
        .materialized_universe()
        .expect("percent validated its materialized pattern universe");
    let queue = problem.core_query().remaining_queue();
    let coverage_source = match queue.mode() {
        "observed" => OBSERVED_MATERIALIZED_PATTERN_SPECIFIC,
        "bag-aligned-pattern" => "bag-aligned-single-pattern",
        "fixed" => "fixed-single-pattern",
        _ => "materialized-pattern-specific",
    };
    let covered_pattern_basis = if source.complete() {
        COVERED_PATTERN_BASIS_COMPLETE_PATTERN_UNIVERSE
    } else {
        COVERED_PATTERN_BASIS_MATERIALIZED_PATTERN_UNIVERSE
    };
    let probability = buildup
        .coverage_probability()
        .parse::<f64>()
        .map(format_probability)
        .map_err(|_| PercentServiceError::InvalidCoverageProbability)?;
    let materialized_probability_mass =
        format_probability(universe.materialized_probability_mass().get());
    let verified_pattern_count = if packing.count_complete() && buildup.count_complete() {
        universe.pattern_count()
    } else {
        0
    };

    let mut fields = vec![
        field("status", "percent-executed"),
        field("execution_scope", "m18-cli-product-path"),
        field("product_slice", "M26 Percent / Path Product Slice"),
        field("percent_workflow", "queue pattern universe -> multiset-grouped C Packing -> pattern-specific C BuildUp coverage rows -> PatternBitSet union -> weighted probability"),
        field("executor_flow", "SearchProblem->C PackingProblem->C PackingResult->C BuildUpResult->CoverageRows->Rust ObjectiveResult->Rust OutputModel"),
        field("problem_layer", "clearra-problem"),
        field("problem_preset", problem.preset().as_str()),
        field("problem_source", problem.scenario().source().as_str()),
        field("compiled_goal", problem.goal().as_str()),
        field("compiled_piece_window", problem.piece_window().max_pieces()),
        field("executor_layer", "clearra-core-executor"),
        field("coverage_row_source", "c-coverage-row-view"),
        field("coverage_reducer", "pattern-bitset-union"),
        field("rust_objective_reducer", "ObjectiveReducer::reduce"),
        field("rust_output_model", "CoreExecutionResult"),
        field("queue_mode", queue.mode()),
        field("queue_len", queue.len()),
        field("minimum_len", problem.piece_window().max_pieces()),
        field("max_patterns", problem.budget().max_patterns()),
        field("route", "search-problem-core-executor"),
        field("piece_source_id", source.id().get()),
        field("pattern_universe_id", universe.pattern_universe_id().get()),
        field("pattern_weight_model_id", universe.pattern_weight_model_id().get()),
        field("packing_multiset_group_count", packing.multiset_group_count()),
        field("pattern_count", universe.pattern_count()),
        field("coverage_pattern_count", universe.pattern_count()),
        field("verified_pattern_count", verified_pattern_count),
        field("materialized_pattern_count", universe.pattern_count()),
        field("total_pattern_count", universe.total_possible_pattern_count()),
        field("covered_pattern_count", buildup.covered_pattern_count()),
        field("covered_pattern_count_basis", covered_pattern_basis),
        field("weighted_pattern_count", universe.weights().len()),
        field("coverage_source", coverage_source),
        field("c_buildup_coverage_row_count", buildup.coverage_row_count()),
        field("pattern_bitset_union", "PatternBitSet OR union"),
        field("weighted_probability_reducer", "union_probability"),
        field("probability", &probability),
        field("weighted_probability", &probability),
        field("materialized_probability_mass", &materialized_probability_mass),
        field("probability_complete", probability_complete),
        field("count_complete", probability_complete),
        field("count_truncated_reason", truncation_reason),
        field("truncated", !probability_complete),
        field("renormalized", false),
        field("truncation_reason", truncation_reason),
        field("resource_truncated", resource_report.truncated),
        field("resource_truncation_reason", truncation_reason),
        field("resource_peak_frontier_states", resource_report.peak_frontier_states),
        field("resource_peak_candidate_rows", resource_report.peak_candidate_rows),
        field("resource_peak_hash_buckets", resource_report.peak_hash_buckets),
        field("resource_peak_gpu_bytes", resource_report.peak_gpu_bytes),
        field("resource_peak_cpu_bytes", resource_report.peak_cpu_bytes),
        field(
            "resource_buildup_workspace_bytes",
            buildup.peak_workspace_bytes(),
        ),
        field("resource_build_worker_backlog_peak", resource_report.build_worker_backlog_peak),
        field("resource_coverage_rows_emitted", resource_report.coverage_rows_emitted),
        field("resource_probability_complete", probability_complete),
        field("percent_reports_total_pattern_count", true),
        field("percent_reports_covered_pattern_count", true),
        field("percent_reports_probability_complete", true),
        field("coverage_probability", &probability),
    ];
    if let Some(observed) = source.observed_window_descriptor() {
        fields.push(field("observed_pattern_budget", observed.budget()));
    }
    Ok(CoreExecutionResult::new(fields, Vec::new()))
}

fn truncation_reason(
    problem: &SearchProblem,
    packing: &crate::packing::PackingRunResult,
    buildup: &crate::buildup::BuildUpRunResult,
) -> &'static str {
    if !problem.piece_source().complete() {
        return "observed_universe_truncated";
    }
    packing
        .truncation_reason()
        .map(ResourceTruncationReason::as_str)
        .unwrap_or_else(|| buildup.count_truncated_reason())
}

fn format_probability(value: f64) -> String {
    if value == 0.0 || value == 1.0 {
        return format!("{value:.0}");
    }
    format!("{value:.12}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

#[cfg(all(test, feature = "native-c-core"))]
#[path = "percent_service_tests.rs"]
mod tests;
