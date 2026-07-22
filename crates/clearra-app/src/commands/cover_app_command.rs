use clearra_build_coverage::{
    query::build_coverage_query::BuildCoverageQuery,
    template::{TemplateExport, TemplateExportFormat},
};
use clearra_problem::{BuildProblemLimits, BuildQuery, BuildTemplateBridge, ProblemCompiler};
use clearra_validation::validators::build_query_validator::validate_build_coverage_query;

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    commands::execution_error_response::core_execution_error_response,
    render::{AppMessage, AppRenderModel, AppResultKind},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoverAppCommand {
    query: BuildCoverageQuery,
    export_template_json: bool,
}

impl CoverAppCommand {
    pub fn new(query: BuildCoverageQuery) -> Self {
        Self {
            query,
            export_template_json: false,
        }
    }
}
impl CoverAppCommand {
    pub fn with_export_template_json(mut self, export_template_json: bool) -> Self {
        self.export_template_json = export_template_json;
        self
    }
}
impl CoverAppCommand {
    pub fn query(&self) -> &BuildCoverageQuery {
        &self.query
    }
}

impl RunnableAppCommand for CoverAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        let report = validate_build_coverage_query(&self.query);
        if report.has_errors() {
            return AppResponse::validation_failed(report);
        }
        if self.export_template_json {
            return match TemplateExport::new(
                "app export_template_json",
                TemplateExportFormat::Json,
                self.query.template().clone(),
            )
            .to_json()
            {
                Ok(json) => AppResponse::success(AppRenderModel::CoverMessage(AppMessage::raw(
                    AppResultKind::Cover,
                    json,
                ))),
                Err(error) => AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(AppErrorCode::ExecutionFailed, error.to_string()),
                ),
            };
        }

        let problem_query = build_problem_query(&self.query);
        let problem = match ProblemCompiler::compile_build(&problem_query) {
            Ok(problem) => problem,
            Err(error) => {
                return AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(AppErrorCode::ProblemCompileFailed, format!("{error:?}")),
                )
            }
        };
        match context
            .services()
            .core_executor()
            .execute_build_coverage_with_control(&problem, &self.query, context.execution_control())
        {
            Ok(result) => AppResponse::success(AppRenderModel::Cover(result)),
            Err(error) => core_execution_error_response(error),
        }
    }
}

fn build_problem_query(query: &BuildCoverageQuery) -> BuildQuery {
    let template = query.template();
    let mut bridge =
        BuildTemplateBridge::new(template.id(), template.board_size(), template.slots().len());
    if let Some(label) = template.label() {
        bridge = bridge.with_label(label);
    }
    let limits = query.limits();

    BuildQuery::coverage_bridge(
        bridge,
        query.pattern_count(),
        BuildProblemLimits::new(limits.max_assignments(), limits.max_patterns()),
    )
}
