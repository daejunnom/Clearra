use clearra_core_executor::CoreExecutionResult;
use clearra_pc_graph::request::OpeningPcSearchQuery;
use clearra_problem::ProblemCompiler;
use clearra_validation::validators::pc_query_validator::validate_opening_pc_search_query;

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    commands::{
        bool_field, execution_error_response::core_execution_error_response, number_field,
        string_field,
    },
    render::{AppMessage, AppRenderModel, AppResultKind},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathAppCommand {
    query: OpeningPcSearchQuery,
}

impl PathAppCommand {
    pub fn new(query: OpeningPcSearchQuery) -> Self {
        Self { query }
    }
}
impl PathAppCommand {
    pub fn query(&self) -> &OpeningPcSearchQuery {
        &self.query
    }
}

impl RunnableAppCommand for PathAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        let report = validate_opening_pc_search_query(&self.query);
        if report.has_errors() {
            return AppResponse::validation_failed(report);
        }
        let problem = match ProblemCompiler::compile_opening_pc(&self.query) {
            Ok(problem) => problem,
            Err(error) => {
                return AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(AppErrorCode::ProblemCompileFailed, format!("{error:?}")),
                )
            }
        };
        let result = match context
            .services()
            .core_executor()
            .execute_with_control(&problem, context.execution_control())
        {
            Ok(result) => result,
            Err(error) => return core_execution_error_response(error),
        };
        path_response(&self.query, result)
    }
}

pub(crate) fn path_response(
    query: &OpeningPcSearchQuery,
    result: CoreExecutionResult,
) -> AppResponse {
    if result.solution_found() && result.path_steps().is_empty() {
        return AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(
                    AppErrorCode::TraceUnavailable,
                    "path command requires sample_trace_available=true; retained_trace_count=0; trace_retained=false",
                ),
            );
    }
    if result.path_steps().is_empty() {
        return AppResponse::failed(
                AppStatus::ExecutionFailed,
                AppError::new(
                    AppErrorCode::NoSolution,
                    "path command needs at least one retained solution trace; sample_trace_available=false",
                ),
            );
    }

    AppResponse::success(AppRenderModel::Path(AppMessage::new(
        AppResultKind::Path,
        path_fields(query, &result),
    )))
}

fn path_fields(
    query: &OpeningPcSearchQuery,
    result: &CoreExecutionResult,
) -> Vec<clearra_output::model::RenderField> {
    let mut fields = vec![
        string_field("status", "path-rendered"),
        string_field("product_slice", "M26 Percent / Path Product Slice"),
        string_field(
            "path_workflow",
            "SearchProblem -> C Packing / BuildUp -> representative replay -> retained trace -> output",
        ),
        string_field("route", "search-problem-core-executor"),
        number_field("lines", query.target().lines()),
        bool_field("solution_found", result.solution_found()),
        bool_field("sample_trace_available", result.sample_trace_available()),
        bool_field("path_reports_representative_trace", true),
        bool_field("retained_representative_trace", !result.path_steps().is_empty()),
        string_field("representative_trace_source", "retained-trace"),
        bool_field("path_distinguishes_retained_trace_from_total_count", true),
        string_field("total_solution_count", field_or(result, "total_solution_count", "0")),
        string_field("unique_solution_count", field_or(result, "unique_solution_count", "0")),
        string_field("retained_trace_count", field_or(result, "retained_trace_count", "0")),
        string_field("solution_trace_count", field_or(result, "solution_trace_count", "0")),
        string_field("count_complete", field_or(result, "count_complete", "false")),
        string_field(
            "trace_retention_truncated",
            field_or(result, "trace_retention_truncated", "false"),
        ),
        string_field(
            "trace_retention_reason",
            field_or(result, "trace_retention_reason", "none"),
        ),
        string_field("score_post_processing", field_or(result, "score_post_processing", "false")),
        string_field("score_profile", field_or(result, "score_profile", "none")),
        string_field(
            "score_accuracy_level",
            field_or(result, "score_accuracy_level", "none"),
        ),
        string_field(
            "score_best_score",
            field_or(result, "score_best_score", "none"),
        ),
        string_field(
            "score_best_attack",
            field_or(result, "score_best_attack", "none"),
        ),
        string_field(
            "score_does_not_change_probability_union",
            field_or(result, "score_does_not_change_probability_union", "true"),
        ),
        string_field(
            "placement_event_available",
            field_or(result, "placement_event_available", "false"),
        ),
        string_field("clear_event_available", field_or(result, "clear_event_available", "false")),
        string_field(
            "drop_event_basis_available",
            field_or(result, "drop_event_basis_available", "false"),
        ),
        string_field(
            "spin_event_basis_available",
            field_or(result, "spin_event_basis_available", "false"),
        ),
        number_field("partition_index", 0),
        number_field("checkpoint_count", 1),
        number_field("placed_pieces", result.path_steps().len()),
        number_field("trace_steps", result.path_steps().len()),
    ];
    for (index, step) in result.path_steps().iter().enumerate() {
        fields.push(string_field(
            format!("step_{index}_piece"),
            step.piece().as_ascii(),
        ));
        fields.push(number_field(
            format!("step_{index}_rotation"),
            step.rotation(),
        ));
        fields.push(number_field(format!("step_{index}_x"), step.x()));
        fields.push(number_field(format!("step_{index}_y"), step.y()));
        fields.push(string_field(format!("step_{index}_hold"), step.hold()));
        fields.push(number_field(
            format!("step_{index}_cleared_lines"),
            step.cleared_lines(),
        ));
    }
    fields
}

fn field_or<'a>(result: &'a CoreExecutionResult, key: &str, default: &'a str) -> &'a str {
    result.field(key).unwrap_or(default)
}
