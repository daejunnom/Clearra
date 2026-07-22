use clearra_pc_graph::request::{PcQueueInput, PcScenarioQuery};
use clearra_problem::ProblemCompiler;
use clearra_validation::{
    diagnostic::diagnostic_report::DiagnosticReport,
    validators::supply_validator::{
        validate_bag_aligned_pattern, validate_fixed_sequence, validate_observed_queue,
    },
};

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    commands::execution_error_response::percent_execution_error_response,
    render::AppRenderModel,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PercentAppCommand {
    query: PcScenarioQuery,
}

impl PercentAppCommand {
    pub fn new(query: PcScenarioQuery) -> Self {
        Self { query }
    }
}
impl PercentAppCommand {
    pub fn query(&self) -> &PcScenarioQuery {
        &self.query
    }
}
impl PercentAppCommand {
    pub fn validate(&self) -> DiagnosticReport {
        validate_percent_queue(self.query.remaining_queue())
    }
}

impl RunnableAppCommand for PercentAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        let report = self.validate();
        if report.has_errors() {
            return AppResponse::validation_failed(report);
        }

        let problem = match ProblemCompiler::compile_scenario_pc(&self.query) {
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
            .execute_percent_with_control(&problem, context.execution_control())
        {
            Ok(result) => AppResponse::success(AppRenderModel::Percent(result)),
            Err(error) => percent_execution_error_response(error),
        }
    }
}

fn validate_percent_queue(queue: &PcQueueInput) -> DiagnosticReport {
    match queue {
        PcQueueInput::FixedSequence(sequence) => validate_fixed_sequence(sequence),
        PcQueueInput::BagAlignedPattern(pattern) => validate_bag_aligned_pattern(pattern),
        PcQueueInput::PatternExpression(_) => DiagnosticReport::new(),
        PcQueueInput::Standard7Bag => DiagnosticReport::new(),
        PcQueueInput::Observed(queue) => validate_observed_queue(queue),
    }
}
