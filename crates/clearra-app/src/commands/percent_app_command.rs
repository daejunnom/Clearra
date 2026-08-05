use clearra_core_executor::CoreExecutionResult;
use clearra_coverage::pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId};
use clearra_pc_graph::request::{OpeningPcSearchQuery, PcQueueInput, PcScenarioQuery};
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
    query: PercentSearchQuery,
    failed_pattern_limit: usize,
    result_mode: PercentResultMode,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PercentSearchQuery {
    Opening(OpeningPcSearchQuery),
    Scenario(PcScenarioQuery),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PercentResultMode {
    Percent,
    FailedQueue,
}

impl PercentAppCommand {
    pub fn new(query: PcScenarioQuery) -> Self {
        Self {
            query: PercentSearchQuery::Scenario(query),
            failed_pattern_limit: 100,
            result_mode: PercentResultMode::Percent,
        }
    }

    pub fn failed_queue(query: PcScenarioQuery) -> Self {
        Self {
            query: PercentSearchQuery::Scenario(query),
            failed_pattern_limit: usize::MAX,
            result_mode: PercentResultMode::FailedQueue,
        }
    }

    pub fn failed_queue_opening(query: OpeningPcSearchQuery) -> Self {
        Self {
            query: PercentSearchQuery::Opening(query),
            failed_pattern_limit: usize::MAX,
            result_mode: PercentResultMode::FailedQueue,
        }
    }

    pub fn with_failed_pattern_limit(mut self, failed_pattern_limit: usize) -> Self {
        self.failed_pattern_limit = failed_pattern_limit;
        self
    }
}
impl PercentAppCommand {
    pub fn query(&self) -> Option<&PcScenarioQuery> {
        match &self.query {
            PercentSearchQuery::Scenario(query) => Some(query),
            PercentSearchQuery::Opening(_) => None,
        }
    }

    pub fn opening_query(&self) -> Option<&OpeningPcSearchQuery> {
        match &self.query {
            PercentSearchQuery::Opening(query) => Some(query),
            PercentSearchQuery::Scenario(_) => None,
        }
    }

    pub const fn failed_pattern_limit(&self) -> usize {
        self.failed_pattern_limit
    }

    pub const fn is_failed_queue(&self) -> bool {
        matches!(self.result_mode, PercentResultMode::FailedQueue)
    }

    pub fn requested_backend(&self) -> &'static str {
        match &self.query {
            PercentSearchQuery::Opening(query) => {
                query.execution_policy().requested_backend().as_str()
            }
            PercentSearchQuery::Scenario(query) => {
                query.execution_policy().requested_backend().as_str()
            }
        }
    }

    pub fn allow_backend_fallback(&self) -> bool {
        match &self.query {
            PercentSearchQuery::Opening(query) => query.execution_policy().allow_backend_fallback(),
            PercentSearchQuery::Scenario(query) => {
                query.execution_policy().allow_backend_fallback()
            }
        }
    }

    pub fn gpu_device_display(&self) -> String {
        match &self.query {
            PercentSearchQuery::Opening(query) => {
                query.execution_policy().gpu_device().as_display_string()
            }
            PercentSearchQuery::Scenario(query) => {
                query.execution_policy().gpu_device().as_display_string()
            }
        }
    }
}
impl PercentAppCommand {
    pub fn validate(&self) -> DiagnosticReport {
        match &self.query {
            PercentSearchQuery::Opening(query) => {
                clearra_validation::validators::pc_query_validator::validate_opening_pc_search_query(
                    query,
                )
            }
            PercentSearchQuery::Scenario(query)
                if matches!(self.result_mode, PercentResultMode::FailedQueue) =>
            {
                clearra_validation::validators::pc_query_validator::validate_pc_scenario_query(
                    query,
                )
            }
            PercentSearchQuery::Scenario(query) => validate_percent_queue(query.remaining_queue()),
        }
    }
}

impl RunnableAppCommand for PercentAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        let report = self.validate();
        if report.has_errors() {
            return AppResponse::validation_failed(report);
        }

        let problem = match &self.query {
            PercentSearchQuery::Opening(query) => ProblemCompiler::compile_opening_percent(query),
            PercentSearchQuery::Scenario(query) => ProblemCompiler::compile_scenario_percent(query),
        };
        let problem = match problem {
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
                if matches!(self.result_mode, PercentResultMode::FailedQueue)
                    && !result.bool_field("probability_complete").unwrap_or(false)
                {
                    return AppResponse::failed(
                        AppStatus::ExecutionFailed,
                        AppError::new(
                            AppErrorCode::ExecutionFailed,
                            "failed_queue_coverage_incomplete",
                        ),
                    );
                }
                match decorate_percent_result(
                    &problem,
                    result,
                    self.failed_pattern_limit,
                    self.result_mode,
                ) {
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
    result_mode: PercentResultMode,
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
    if matches!(result_mode, PercentResultMode::FailedQueue) {
        fields.extend([
            ("result_mode".to_owned(), "failed-queue".to_owned()),
            (
                "failed_queue_contract".to_owned(),
                "exact-coverage-complement".to_owned(),
            ),
            (
                "failed_queue_probability".to_owned(),
                complement_probability(result.field("coverage_probability").unwrap_or("0")),
            ),
        ]);
    }
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
        ("failed_pattern_limit".to_owned(), example_limit.to_string()),
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

fn complement_probability(value: &str) -> String {
    let Ok(probability) = value.parse::<f64>() else {
        return "0".to_owned();
    };
    let complement = (1.0 - probability).clamp(0.0, 1.0);
    let rendered = format!("{complement:.12}");
    rendered
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
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

        let result = decorate_percent_result(&problem, result, 1, PercentResultMode::Percent)
            .expect("decorated");

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

    #[test]
    fn failed_queue_decorator_marks_the_exact_complement_contract() {
        let query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(1, 0x3f0),
            PcQueueInput::observed(ObservedQueue::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        );
        let problem = ProblemCompiler::compile_scenario_percent(&query).expect("problem");
        let pattern_count = problem
            .piece_source()
            .materialized_universe()
            .expect("universe")
            .pattern_count();
        let result = CoreExecutionResult::new(
            vec![
                ("coverage_probability".to_owned(), "0.25".to_owned()),
                ("probability_complete".to_owned(), "true".to_owned()),
            ],
            Vec::new(),
        )
        .with_coverage_pattern_words(vec![0; pattern_count.div_ceil(u64::BITS as usize)]);

        let result =
            decorate_percent_result(&problem, result, usize::MAX, PercentResultMode::FailedQueue)
                .expect("decorated");

        assert_eq!(result.field("result_mode"), Some("failed-queue"));
        assert_eq!(
            result.field("failed_queue_contract"),
            Some("exact-coverage-complement")
        );
        assert_eq!(result.field("failed_queue_probability"), Some("0.75"));
        assert_eq!(
            result.usize_field("failed_pattern_examples_materialized"),
            Some(pattern_count)
        );
    }

    #[cfg(feature = "native-c-core")]
    #[test]
    fn opening_failed_queue_runs_with_exact_coverage_words() {
        use clearra_core_domain::pc::pc_target::PcTarget;
        use clearra_supply::queue::fixed_sequence::FixedSequence;

        use crate::{AppCommand, AppContext, AppRequest, AppStatus};

        let query = OpeningPcSearchQuery::new(PcTarget::two_lines())
            .with_queue(PcQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
                PieceKind::I,
                PieceKind::O,
                PieceKind::O,
                PieceKind::O,
            ])))
            .with_hold_policy(clearra_pc_graph::request::PcHoldPolicy::Disabled);

        let response = AppContext::default().run(AppRequest::new(AppCommand::Percent(
            PercentAppCommand::failed_queue_opening(query),
        )));

        assert_eq!(response.status(), AppStatus::Success, "{response:#?}");
        let Some(AppRenderModel::Percent(result)) = response.render_model() else {
            panic!("percent render model");
        };
        assert_eq!(result.field("result_mode"), Some("failed-queue"));
        assert_eq!(result.field("problem_preset"), Some("opening-pc"));
        assert_eq!(result.field("probability_complete"), Some("true"));
        assert_eq!(result.field("failed_pattern_count"), Some("0"));
        assert_eq!(result.field("failed_queue_probability"), Some("0"));
        assert_eq!(result.coverage_pattern_words(), &[1]);
    }
}
