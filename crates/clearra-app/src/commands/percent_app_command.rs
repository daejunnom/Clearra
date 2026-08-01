use clearra_core_executor::CoreExecutionResult;
use clearra_coverage::pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId};
use clearra_pc_graph::request::{PcQueueInput, PcScenarioQuery};
use clearra_problem::{ProblemCompiler, SearchProblem};
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
    commands::execution_error_response::core_execution_error_response,
    render::AppRenderModel,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PercentAppCommand {
    query: PcScenarioQuery,
    failed_pattern_limit: usize,
}

impl PercentAppCommand {
    pub fn new(query: PcScenarioQuery) -> Self {
        Self {
            query,
            failed_pattern_limit: 100,
        }
    }

    pub fn with_failed_pattern_limit(mut self, failed_pattern_limit: usize) -> Self {
        self.failed_pattern_limit = failed_pattern_limit;
        self
    }
}
impl PercentAppCommand {
    pub fn query(&self) -> &PcScenarioQuery {
        &self.query
    }

    pub const fn failed_pattern_limit(&self) -> usize {
        self.failed_pattern_limit
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

        let problem = match ProblemCompiler::compile_scenario_percent(&self.query) {
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
            .execute_with_control(&problem, context.execution_control())
        {
            Ok(result) => {
                match decorate_percent_result(&problem, result, self.failed_pattern_limit) {
                    Ok(result) => AppResponse::success(AppRenderModel::Percent(result)),
                    Err(reason) => AppResponse::failed(
                        AppStatus::ExecutionFailed,
                        AppError::new(AppErrorCode::ExecutionFailed, reason),
                    ),
                }
            }
            Err(error) => core_execution_error_response(error),
        }
    }
}

fn decorate_percent_result(
    problem: &SearchProblem,
    result: CoreExecutionResult,
    failed_pattern_limit: usize,
) -> Result<CoreExecutionResult, &'static str> {
    let universe = problem
        .piece_source()
        .materialized_universe()
        .ok_or("percent_pattern_universe_unavailable")?;
    let coverage = PatternBitSet::from_words(
        universe.pattern_count(),
        result.coverage_pattern_words().to_vec(),
    )
    .map_err(|_| "percent_coverage_pattern_words_invalid")?;
    let failed_pattern_count = universe
        .pattern_count()
        .saturating_sub(coverage.count_ones() as usize);
    let example_limit = failed_pattern_limit.min(failed_pattern_count);
    let failed_pattern_count_complete = result.bool_field("probability_complete").unwrap_or(false);
    let mut fields = Vec::with_capacity(example_limit.saturating_add(8));
    fields.extend([
        (
            "percent_evidence_contract".to_owned(),
            "coverage-summary".to_owned(),
        ),
        (
            "total_pattern_count".to_owned(),
            universe.pattern_count().to_string(),
        ),
        (
            "probability".to_owned(),
            result
                .field("coverage_probability")
                .unwrap_or("0")
                .to_owned(),
        ),
        (
            "failed_pattern_count".to_owned(),
            failed_pattern_count.to_string(),
        ),
        (
            "failed_pattern_scope".to_owned(),
            "materialized-universe".to_owned(),
        ),
        (
            "failed_pattern_count_complete".to_owned(),
            failed_pattern_count_complete.to_string(),
        ),
        (
            "failed_pattern_limit".to_owned(),
            failed_pattern_limit.to_string(),
        ),
    ]);
    let mut example_count = 0usize;
    for pattern_index in 0..universe.pattern_count() {
        if example_count == example_limit {
            break;
        }
        if coverage.contains(PatternId::new(pattern_index)) {
            continue;
        }
        let sequence = universe
            .sequence_at(pattern_index)
            .iter()
            .map(|piece| piece.as_ascii())
            .collect::<String>();
        fields.push((format!("failed_pattern_{example_count}"), sequence));
        example_count += 1;
    }
    fields.extend([
        (
            "failed_pattern_examples_materialized".to_owned(),
            example_count.to_string(),
        ),
        (
            "failed_pattern_examples_truncated".to_owned(),
            (example_count < failed_pattern_count).to_string(),
        ),
    ]);
    Ok(result.with_additional_fields(fields))
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

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;
    use clearra_pc_graph::request::{PcScenarioBoard, PieceWindow};
    use clearra_supply::queue::observed_queue::ObservedQueue;

    use super::*;

    #[test]
    fn percent_decorator_reports_bounded_failed_sequences() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(1, 0x3f0),
            PcQueueInput::observed(ObservedQueue::new(vec![PieceKind::I, PieceKind::O])),
            PieceWindow::new(2),
        );
        let problem = ProblemCompiler::compile_scenario_percent(&query).expect("problem");
        let pattern_count = problem
            .piece_source()
            .materialized_universe()
            .expect("universe")
            .pattern_count();
        let result = CoreExecutionResult::new(
            vec![("coverage_probability".to_owned(), "0".to_owned())],
            Vec::new(),
        )
        .with_coverage_pattern_words(vec![0; pattern_count.div_ceil(u64::BITS as usize)]);

        let result = decorate_percent_result(&problem, result, 1).expect("decorated");

        assert_eq!(
            result.field("percent_evidence_contract"),
            Some("coverage-summary")
        );
        assert_eq!(
            result.usize_field("failed_pattern_count"),
            Some(pattern_count)
        );
        assert_eq!(
            result.usize_field("failed_pattern_examples_materialized"),
            Some(1)
        );
        assert_eq!(
            result.field("failed_pattern_scope"),
            Some("materialized-universe")
        );
        assert_eq!(result.field("failed_pattern_count_complete"), Some("false"));
        assert_eq!(
            result.field("failed_pattern_examples_truncated"),
            Some("true")
        );
        assert!(result.field("failed_pattern_0").is_some());
    }
}
