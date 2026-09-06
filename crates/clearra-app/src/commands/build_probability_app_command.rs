use clearra_problem::{BuildProbabilityQuery, ProblemCompiler};

use clearra_core_domain::execution_cancellation::ExecutionControl;
use clearra_core_executor::{CoreExecutionError, CoreExecutionResult};

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    build_probability_product_result::{
        build_complete_replay_payload, build_field_average_payload,
        build_fixed_queue_max_score_payload, build_highest_score_minimum_payload,
        decorate_build_failed_queues,
    },
    build_solution_probability_result::build_probability_response,
    commands::execution_error_response::core_execution_error_response,
    pc_score_postprocess::PcScoreDerivation,
    AppCoreExecutorService,
};

/// Product-level result aggregation for one Build probability execution.
///
/// This is intentionally independent of `BuildProbabilityAggregation`, which
/// selects buildability/geometry/spin execution semantics.  Keeping the two
/// axes separate prevents a GUI label from silently changing the probability
/// denominator or the requested target field.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BuildProbabilityResultMode {
    #[default]
    AllSolutions,
    CompleteReplayPaths,
    FieldAverageScore,
    FixedQueueMaximumScore,
    HighestScoreMinimumSet,
    FailedQueues,
}

impl BuildProbabilityResultMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AllSolutions => "all-solutions",
            Self::CompleteReplayPaths => "complete-replay-paths",
            Self::FieldAverageScore => "field-average-score",
            Self::FixedQueueMaximumScore => "fixed-queue-maximum-score",
            Self::HighestScoreMinimumSet => "highest-score-minimum-set",
            Self::FailedQueues => "failed-queues",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BuildProbabilityAppCommand {
    query: BuildProbabilityQuery,
    result_mode: BuildProbabilityResultMode,
    failed_pattern_limit: usize,
    product_retention_budget: Option<crate::ProductRetentionBudget>,
}

impl BuildProbabilityAppCommand {
    pub fn new(query: BuildProbabilityQuery) -> Self {
        Self {
            query,
            result_mode: BuildProbabilityResultMode::AllSolutions,
            failed_pattern_limit: 100,
            product_retention_budget: None,
        }
    }

    pub const fn with_result_mode(mut self, result_mode: BuildProbabilityResultMode) -> Self {
        self.result_mode = result_mode;
        self
    }

    pub fn query(&self) -> &BuildProbabilityQuery {
        &self.query
    }

    pub const fn result_mode(&self) -> BuildProbabilityResultMode {
        self.result_mode
    }

    pub(crate) fn set_product_retention_budget(
        &mut self,
        budget: Option<crate::ProductRetentionBudget>,
    ) {
        self.product_retention_budget = budget;
    }

    pub const fn with_failed_pattern_limit(mut self, failed_pattern_limit: usize) -> Self {
        self.failed_pattern_limit = failed_pattern_limit;
        self
    }

    pub const fn failed_pattern_limit(&self) -> usize {
        self.failed_pattern_limit
    }

    pub(crate) const fn requires_score_derivation(&self) -> bool {
        matches!(
            self.result_mode,
            BuildProbabilityResultMode::FieldAverageScore
                | BuildProbabilityResultMode::FixedQueueMaximumScore
                | BuildProbabilityResultMode::HighestScoreMinimumSet
        )
    }

    pub(crate) fn into_query(self) -> BuildProbabilityQuery {
        self.query
    }

    pub(crate) fn invalid_reason(&self) -> Option<&'static str> {
        invalid_query_reason(&self.query)
            .or_else(|| invalid_result_mode_reason(&self.query, self.result_mode))
    }

    /// Materializes only the additional Core evidence required by the chosen
    /// Build result product. The search and ordinary Build public-surface
    /// post-process have already completed before this boundary is entered.
    pub(crate) fn materialize_result_mode_evidence(
        &self,
        core_executor: &AppCoreExecutorService,
        control: &ExecutionControl,
        result: CoreExecutionResult,
    ) -> Result<CoreExecutionResult, CoreExecutionError> {
        if self.result_mode == BuildProbabilityResultMode::CompleteReplayPaths {
            core_executor.materialize_build_terminal_replay_partition(result, control)
        } else {
            Ok(result)
        }
    }

    /// Projects one already executed and fully post-processed Build result.
    /// Keeping this adapter command-owned makes direct, cooperative, and
    /// distributed execution share the same product/decorator semantics.
    pub(crate) fn response_from_materialized_result(
        self,
        result: CoreExecutionResult,
        score_derivation: Option<PcScoreDerivation>,
    ) -> AppResponse {
        let (payload, page_source_owner) = match self.result_mode {
            BuildProbabilityResultMode::AllSolutions => (None, None),
            BuildProbabilityResultMode::CompleteReplayPaths => {
                match build_complete_replay_payload(
                    &self.query,
                    &result,
                    self.product_retention_budget,
                ) {
                    Ok(payload) => (Some(payload), None),
                    Err(reason) => return result_projection_failed_response(reason),
                }
            }
            BuildProbabilityResultMode::FieldAverageScore => {
                let Some(derivation) = score_derivation.as_ref() else {
                    return result_projection_failed_response(
                        "Build field-average typed score evidence is missing",
                    );
                };
                match build_field_average_payload(
                    &self.query,
                    &result,
                    derivation,
                    self.product_retention_budget,
                ) {
                    Ok(payload) => (Some(payload), None),
                    Err(reason) => return result_projection_failed_response(reason),
                }
            }
            BuildProbabilityResultMode::FixedQueueMaximumScore => {
                let Some(derivation) = score_derivation.as_ref() else {
                    return result_projection_failed_response(
                        "Build fixed-queue typed score evidence is missing",
                    );
                };
                match build_fixed_queue_max_score_payload(
                    &self.query,
                    &result,
                    derivation,
                    self.product_retention_budget,
                ) {
                    Ok(payload) => (Some(payload), None),
                    Err(reason) => return result_projection_failed_response(reason),
                }
            }
            BuildProbabilityResultMode::HighestScoreMinimumSet => {
                let Some(derivation) = score_derivation.as_ref() else {
                    return result_projection_failed_response(
                        "Build score-minimum typed score evidence is missing",
                    );
                };
                match build_highest_score_minimum_payload(
                    &self.query,
                    &result,
                    derivation,
                    self.product_retention_budget,
                ) {
                    Ok((payload, owner)) => (Some(payload), Some(owner)),
                    Err(reason) => return result_projection_failed_response(reason),
                }
            }
            BuildProbabilityResultMode::FailedQueues => (None, None),
        };
        let result = if self.result_mode == BuildProbabilityResultMode::FailedQueues {
            match decorate_build_failed_queues(
                &self.query,
                result,
                self.failed_pattern_limit,
                self.product_retention_budget,
            ) {
                Ok(result) => result,
                Err(reason) => return result_projection_failed_response(reason),
            }
        } else {
            result
        };
        let response = build_probability_response(
            self.query.finesse_request(),
            self.query.field(),
            self.query.aggregation(),
            self.query.solution_probability_policy(),
            result,
        );
        match payload {
            Some(payload) if response.status() == AppStatus::Success => {
                response.with_public_product_result(payload, page_source_owner)
            }
            _ => response,
        }
    }
}

impl RunnableAppCommand for BuildProbabilityAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        if let Some(reason) = self.invalid_reason() {
            return AppResponse::failed(
                AppStatus::ValidationFailed,
                AppError::new(AppErrorCode::InvalidInput, reason),
            );
        }
        let problem = match ProblemCompiler::compile_scenario_pc(self.query.core_query()) {
            Ok(problem) => problem,
            Err(error) => {
                return AppResponse::failed(
                    AppStatus::ExecutionFailed,
                    AppError::new(AppErrorCode::ProblemCompileFailed, format!("{error:?}")),
                )
            }
        };
        let execution = if self.requires_score_derivation() {
            context
                .services()
                .core_executor()
                .execute_build_probability_with_score_derivation_with_control(
                    &problem,
                    self.query.field(),
                    self.query.aggregation(),
                    self.query.finesse_request().clone(),
                    self.query.solution_probability_policy(),
                    context.execution_control(),
                )
                .map(|(result, derivation)| (result, Some(derivation)))
        } else {
            context
                .services()
                .core_executor()
                .execute_build_probability_with_control(
                    &problem,
                    self.query.field(),
                    self.query.aggregation(),
                    self.query.finesse_request().clone(),
                    self.query.solution_probability_policy(),
                    context.execution_control(),
                )
                .map(|result| (result, None))
        };
        match execution {
            Ok((result, score_derivation)) => match self.materialize_result_mode_evidence(
                context.services().core_executor(),
                context.execution_control(),
                result,
            ) {
                Ok(result) => self.response_from_materialized_result(result, score_derivation),
                Err(error) => core_execution_error_response(error),
            },
            Err(error) => core_execution_error_response(error),
        }
    }
}

fn result_projection_failed_response(reason: &'static str) -> AppResponse {
    AppResponse::failed(
        AppStatus::ExecutionFailed,
        AppError::new(AppErrorCode::ExecutionFailed, reason),
    )
}

fn invalid_result_mode_reason(
    query: &BuildProbabilityQuery,
    mode: BuildProbabilityResultMode,
) -> Option<&'static str> {
    match mode {
        BuildProbabilityResultMode::AllSolutions => None,
        BuildProbabilityResultMode::CompleteReplayPaths => {
            if query.aggregation().is_tiling_only() {
                Some("complete Build replay paths require reachable Build execution")
            } else if query.finesse_metric().requested() {
                Some("complete Build replay paths cannot be combined with finesse aggregation")
            } else if !query.field().is_compact() {
                Some("complete Build replay paths currently require a compact six-row field")
            } else if !query.core_query().objective().score().requested() {
                Some("complete Build replay paths require exact execution evidence")
            } else {
                None
            }
        }
        BuildProbabilityResultMode::FieldAverageScore => {
            if query.aggregation().is_tiling_only() {
                Some("Build field-average score requires reachable Build execution")
            } else if query.finesse_metric().requested() {
                Some("Build field-average score cannot be combined with finesse aggregation")
            } else if !query.field().is_compact() {
                Some("Build field-average score currently requires a compact six-row field")
            } else if !query.core_query().objective().score().requested() {
                Some("Build field-average score requires score execution evidence")
            } else {
                None
            }
        }
        BuildProbabilityResultMode::FixedQueueMaximumScore => {
            if query.aggregation().is_tiling_only() {
                Some("Build fixed-queue maximum score requires reachable Build execution")
            } else if query.finesse_metric().requested() {
                Some("Build fixed-queue maximum score cannot be combined with finesse aggregation")
            } else if !query.field().is_compact() {
                Some("Build fixed-queue maximum score currently requires a compact six-row field")
            } else if query
                .core_query()
                .remaining_queue()
                .as_fixed_sequence()
                .is_none()
            {
                Some("Build fixed-queue maximum score requires one exact fixed queue")
            } else if !query.core_query().objective().score().requested() {
                Some("Build fixed-queue maximum score requires score execution evidence")
            } else {
                None
            }
        }
        BuildProbabilityResultMode::HighestScoreMinimumSet => {
            if query.aggregation().is_tiling_only() {
                Some("Build highest-score minimum set requires reachable Build execution")
            } else if query.finesse_metric().requested() {
                Some("Build highest-score minimum set cannot be combined with finesse aggregation")
            } else if !query.field().is_compact() {
                Some("Build highest-score minimum set currently requires a compact six-row field")
            } else if !query.core_query().objective().score().requested() {
                Some("Build highest-score minimum set requires score execution evidence")
            } else {
                None
            }
        }
        BuildProbabilityResultMode::FailedQueues => {
            if query.aggregation().is_tiling_only() {
                Some("Build failed queues require reachable Build execution")
            } else if query.finesse_metric().requested() {
                Some("Build failed queues cannot be combined with finesse aggregation")
            } else {
                None
            }
        }
    }
}

pub(crate) fn invalid_query_reason(query: &BuildProbabilityQuery) -> Option<&'static str> {
    let field = query.field();
    if query.aggregation().is_tiling_only()
        && query
            .queue_observation_policy()
            .requires_observation_policy()
    {
        return Some("visible-7 queue knowledge is unavailable with tiling-only Build");
    }
    if query.solution_probability_policy().requested() && query.aggregation().is_tiling_only() {
        return Some("per-solution probabilities are unavailable with tiling-only Build");
    }
    if query.solution_probability_policy().requested() && query.finesse_score().is_some() {
        return Some("per-solution probabilities are unavailable with Build finesse scoring");
    }
    if field.width() != 10 || field.height() == 0 || field.height() > 24 {
        return Some("build probability requires a 10-wide field between 1 and 24 rows");
    }
    if field.target().intersects(field.base()) {
        return Some("build target cells overlap the existing field");
    }
    if query.finesse_score().is_none() && !field.target().count_ones().is_multiple_of(4) {
        return Some("build target cell count must be divisible by four");
    }
    // Finesse query builders normalize completed input rows before either
    // search or score execution. The ordinary BuildUp contract remains
    // unchanged and continues to reject an unnormalized base field.
    if !query.finesse_metric().requested() {
        for row in 0..field.height() {
            let row_mask = clearra_core_domain::board::standard_pc_board::Board256Mask::row(
                10,
                u16::from(field.height()),
                u16::from(row),
            )
            .ok()?;
            if field.base().intersects(row_mask) && field.base().union(row_mask) == field.base() {
                return Some(
                    "existing field contains a completed row and must be normalized first",
                );
            }
        }
    }
    let expected_piece_count = query.finesse_score().map_or_else(
        || query.target_piece_count(),
        |score| score.placements().len(),
    );
    if query.core_query().exact_pieces() != Some(expected_piece_count) {
        return Some("build supply piece count does not match the target area");
    }
    None
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_problem::{
        BuildProbabilityAggregation, BuildProbabilityField, BuildProbabilityQuery,
        BuildSolutionProbabilityPolicy, FinesseMetric, FinessePatternKnowledge, FinessePlacement,
        FinesseScoreRequest,
    };
    use clearra_supply::queue::fixed_sequence::FixedSequence;
    use clearra_supply::QueueObservationPolicy;

    use super::{invalid_query_reason, invalid_result_mode_reason, BuildProbabilityResultMode};

    enum FinesseRequest {
        Off,
        Search,
        Score,
    }

    fn query_with_completed_base(request: FinesseRequest) -> BuildProbabilityQuery {
        let core = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0x3ff),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let target = (1_u64 << 14) | (1_u64 << 15) | (1_u64 << 24) | (1_u64 << 25);
        let field = BuildProbabilityField::from_words_preserving_height(
            4,
            [0x3ff, 0, 0, 0],
            [target, 0, 0, 0],
        )
        .unwrap();
        let query = BuildProbabilityQuery::new(core, field);
        match request {
            FinesseRequest::Off => query,
            FinesseRequest::Search => {
                query.with_finesse(FinesseMetric::Inputs, FinessePatternKnowledge::Oracle)
            }
            FinesseRequest::Score => query.with_finesse_score(
                FinesseScoreRequest::new(vec![FinessePlacement::new(
                    PieceKind::O,
                    RotationState::Zero,
                    4,
                    0,
                )])
                .unwrap(),
            ),
        }
    }

    fn one_piece_query() -> BuildProbabilityQuery {
        let core = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("compact one-piece field");
        BuildProbabilityQuery::new(core, field)
    }

    #[test]
    fn extended_build_score_products_are_rejected_before_search() {
        let core = one_piece_query().core_query().clone();
        let field = BuildProbabilityField::from_words_preserving_height(
            8,
            [0; 4],
            [0xf000_0000_0000_0000, 0, 0, 0],
        )
        .unwrap();
        let query = BuildProbabilityQuery::new(core, field);
        for mode in [
            BuildProbabilityResultMode::CompleteReplayPaths,
            BuildProbabilityResultMode::FieldAverageScore,
            BuildProbabilityResultMode::FixedQueueMaximumScore,
            BuildProbabilityResultMode::HighestScoreMinimumSet,
        ] {
            assert!(invalid_result_mode_reason(&query, mode)
                .unwrap()
                .contains("compact six-row"));
        }
        assert_eq!(
            invalid_result_mode_reason(&query, BuildProbabilityResultMode::AllSolutions),
            None
        );
    }

    #[test]
    fn completed_base_row_is_normalized_for_finesse_search_and_score_only() {
        assert!(invalid_query_reason(&query_with_completed_base(FinesseRequest::Off)).is_some());
        for request in [FinesseRequest::Search, FinesseRequest::Score] {
            let query = query_with_completed_base(request);
            assert_eq!(invalid_query_reason(&query), None);
            assert_eq!(query.field().height(), 4);
            assert!(query.field().base().is_empty());
            assert_eq!(query.field().compact_target_mask(), Some(0xc030));
        }
    }

    #[test]
    fn finesse_initial_clear_does_not_conceal_overlapping_target_cells() {
        let core = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0x3ff),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let field =
            BuildProbabilityField::from_words_preserving_height(4, [0x3ff, 0, 0, 0], [1, 0, 0, 0])
                .unwrap();
        let query = BuildProbabilityQuery::new(core, field)
            .with_finesse(FinesseMetric::Inputs, FinessePatternKnowledge::Oracle);

        assert_eq!(
            invalid_query_reason(&query),
            Some("build target cells overlap the existing field")
        );
    }

    #[test]
    fn per_solution_probabilities_reject_tiling_and_finesse_score_only() {
        let include = BuildSolutionProbabilityPolicy::Include;
        assert_eq!(
            invalid_query_reason(
                &one_piece_query()
                    .with_aggregation(BuildProbabilityAggregation::TilingOnly)
                    .with_solution_probability_policy(include)
            ),
            Some("per-solution probabilities are unavailable with tiling-only Build")
        );
        assert_eq!(
            invalid_query_reason(
                &query_with_completed_base(FinesseRequest::Score)
                    .with_solution_probability_policy(include)
            ),
            Some("per-solution probabilities are unavailable with Build finesse scoring")
        );

        assert_eq!(
            invalid_query_reason(&one_piece_query().with_solution_probability_policy(include)),
            None
        );
        assert_eq!(
            invalid_query_reason(
                &one_piece_query()
                    .with_finesse(FinesseMetric::Inputs, FinessePatternKnowledge::Oracle)
                    .with_solution_probability_policy(include)
            ),
            None
        );
    }

    #[test]
    fn app_boundary_rejects_visible_seven_with_tiling_only() {
        assert_eq!(
            invalid_query_reason(
                &one_piece_query()
                    .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven)
                    .with_aggregation(BuildProbabilityAggregation::TilingOnly)
            ),
            Some("visible-7 queue knowledge is unavailable with tiling-only Build")
        );
        assert_eq!(
            invalid_query_reason(
                &one_piece_query()
                    .with_queue_observation_policy(QueueObservationPolicy::VisibleSeven)
            ),
            None
        );
    }
}
