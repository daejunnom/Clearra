use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_executor::{CoreExecutionError, CoreExecutionResult};
use clearra_host_contract::AppCommandKind;
use clearra_problem::{BuildProbabilityAggregation, BuildProbabilityField, SearchProblem};
use clearra_validation::diagnostic::diagnostic_report::DiagnosticReport;

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppContext,
    app_request::{AppOutputPolicy, AppRequest},
    app_response::AppResponse,
    commands::core_execution_error_response,
    cooperative_execution::{
        compile_search_command, response_from_search, CooperativeSearchResponseKind,
    },
};

pub enum DistributedSearchPreparation {
    Ready(AppResponse),
    Search(PreparedDistributedSearch),
}

pub struct PreparedDistributedSearch {
    context: AppContext,
    problem: SearchProblem,
    response_kind: CooperativeSearchResponseKind,
    command_kind: AppCommandKind,
    output_policy: AppOutputPolicy,
    validation_report: DiagnosticReport,
}

impl AppContext {
    pub fn prepare_distributed_search(&self, request: AppRequest) -> DistributedSearchPreparation {
        let command_kind = request.command_kind();
        let (command, output_policy, _, _) = request.into_parts();
        let validation_report = command.validate();
        if validation_report.has_errors() {
            let response = command
                .validation_failed_response(validation_report.clone())
                .unwrap_or_else(|| AppResponse::validation_failed(validation_report));
            return DistributedSearchPreparation::Ready(self.finalize_response(
                response,
                command_kind,
                &output_policy,
            ));
        }

        let (problem, response_kind) = match compile_search_command(command) {
            Ok(compiled) => compiled,
            Err(response) => {
                return DistributedSearchPreparation::Ready(self.finalize_response(
                    response,
                    command_kind,
                    &output_policy,
                ));
            }
        };
        DistributedSearchPreparation::Search(PreparedDistributedSearch {
            context: self.clone(),
            problem,
            response_kind,
            command_kind,
            output_policy,
            validation_report,
        })
    }
}

impl PreparedDistributedSearch {
    pub fn problem(&self) -> &SearchProblem {
        &self.problem
    }

    pub fn build_probability_request(
        &self,
    ) -> Option<(BuildProbabilityField, BuildProbabilityAggregation)> {
        match self.response_kind {
            CooperativeSearchResponseKind::BuildProbability { field, aggregation } => {
                Some((field, aggregation))
            }
            _ => None,
        }
    }

    pub fn complete(self, result: CoreExecutionResult, control: &ExecutionControl) -> AppResponse {
        let result =
            decorate_distributed_build_probability_tiling_result(&self.response_kind, result);
        let response = match self
            .context
            .services()
            .core_executor()
            .postprocess_search_result(result, control)
        {
            Ok(result) => response_from_search(self.response_kind, result),
            Err(error) => core_execution_error_response(error),
        };
        let response = if self.validation_report.is_empty() {
            response
        } else {
            response.with_validation_diagnostics(self.validation_report)
        };
        self.context
            .finalize_response(response, self.command_kind, &self.output_policy)
    }

    pub fn fail(self, error: CoreExecutionError) -> AppResponse {
        self.context.finalize_response(
            core_execution_error_response(error),
            self.command_kind,
            &self.output_policy,
        )
    }
}

fn decorate_distributed_build_probability_tiling_result(
    response_kind: &CooperativeSearchResponseKind,
    result: CoreExecutionResult,
) -> CoreExecutionResult {
    let CooperativeSearchResponseKind::BuildProbability { field, aggregation } = response_kind
    else {
        return result;
    };
    if !aggregation.is_tiling_only() || result.field("search_kind") == Some("build-probability") {
        return result;
    }
    let Some(base_mask) = field.compact_base_mask() else {
        return result;
    };
    let Some(target_cells) = field.compact_target_mask() else {
        return result;
    };
    let Some(final_board) = field.compact_final_board_mask() else {
        return result;
    };
    let mirror_included = field.includes_applicable_horizontal_mirror();
    let solution_count = result.usize_field("unique_solution_count").unwrap_or(0);
    let mirror_distinct = result
        .bool_field("build_mirror_distinct_target")
        .unwrap_or(false);
    let mirror_search_executed = result
        .bool_field("build_mirror_search_executed")
        .unwrap_or(mirror_distinct);
    let mirror_solution_count = result
        .usize_field("mirror_unique_solution_count")
        .unwrap_or(if mirror_included { solution_count } else { 0 });
    let mirror_candidate_count = result
        .usize_field("mirror_packing_candidate_count")
        .unwrap_or(0);
    let solution_hash = result
        .field("normalized_solution_set_hash")
        .unwrap_or("not-calculated")
        .to_owned();
    let mirror_solution_hash = result
        .field("mirror_normalized_solution_set_hash")
        .map(str::to_owned)
        .unwrap_or_else(|| {
            if mirror_included {
                solution_hash.clone()
            } else {
                "not-calculated".to_owned()
            }
        });
    result.with_replaced_fields(vec![
        text_field("search_kind", "build-probability"),
        text_field(
            "build_probability_completion",
            "exact-board-with-inverse-lock-clear",
        ),
        text_field("build_base_mask", base_mask),
        text_field("build_target_cells_mask", target_cells),
        text_field("build_target_board_mask", base_mask | target_cells),
        text_field("build_final_board_mask", final_board),
        text_field("target_piece_count", field.target_piece_count()),
        text_field("objective", "build-probability"),
        text_field("build_probability_aggregation", aggregation.as_str()),
        text_field("build_probability_evaluation_basis", "geometry-only"),
        text_field("build_path_multiplicity_counted", false),
        text_field("buildability_verified", false),
        text_field("coverage_calculated", false),
        text_field("probability_calculated", false),
        text_field(
            "build_symmetry_policy",
            if mirror_included {
                "original-or-horizontal-mirror"
            } else {
                "original-only"
            },
        ),
        text_field("build_mirror_included", mirror_included),
        text_field("build_mirror_distinct_target", mirror_distinct),
        text_field("build_mirror_search_executed", mirror_search_executed),
        text_field(
            "solution_count_basis",
            if mirror_included {
                "original-or-horizontal-mirror-union"
            } else {
                "original-field"
            },
        ),
        text_field("coverage_basis", "not-evaluated-tiling-only"),
        text_field("original_covered_pattern_count", 0),
        text_field("original_coverage_probability", "not-calculated"),
        text_field("mirror_covered_pattern_count", 0),
        text_field("mirror_coverage_probability", "not-calculated"),
        text_field("mirror_union_added_pattern_count", 0),
        text_field("mirror_unique_solution_count", mirror_solution_count),
        text_field("mirror_packing_candidate_count", mirror_candidate_count),
        text_field("mirror_normalized_solution_set_hash", mirror_solution_hash),
    ])
}

fn text_field(key: impl Into<String>, value: impl ToString) -> (String, String) {
    (key.into(), value.to_string())
}
