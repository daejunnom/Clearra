use clearra_problem::{BuildProbabilityQuery, ProblemCompiler};

use crate::{
    app_command::RunnableAppCommand,
    app_context::AppExecutionContext,
    app_error::{AppError, AppErrorCode},
    app_response::{AppResponse, AppStatus},
    commands::execution_error_response::core_execution_error_response,
    render::AppRenderModel,
};

#[derive(Clone, Debug, PartialEq)]
pub struct BuildProbabilityAppCommand {
    query: BuildProbabilityQuery,
}

impl BuildProbabilityAppCommand {
    pub fn new(query: BuildProbabilityQuery) -> Self {
        Self { query }
    }

    pub fn query(&self) -> &BuildProbabilityQuery {
        &self.query
    }
}

impl RunnableAppCommand for BuildProbabilityAppCommand {
    fn run(self, context: &AppExecutionContext<'_>) -> AppResponse {
        if let Some(reason) = invalid_query_reason(&self.query) {
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
        match context
            .services()
            .core_executor()
            .execute_build_probability_with_control(
                &problem,
                self.query.field(),
                self.query.aggregation(),
                self.query.finesse_request().clone(),
                context.execution_control(),
            ) {
            Ok(result) => AppResponse::success(AppRenderModel::BuildProbability(result)),
            Err(error) => core_execution_error_response(error),
        }
    }
}

pub(crate) fn invalid_query_reason(query: &BuildProbabilityQuery) -> Option<&'static str> {
    let field = query.field();
    if field.width() != 10 || field.height() == 0 || field.height() > 24 {
        return Some("build probability requires a 10-wide field between 1 and 24 rows");
    }
    if field.target().intersects(field.base()) {
        return Some("build target cells overlap the existing field");
    }
    if query.finesse_score().is_none() && field.target().count_ones() % 4 != 0 {
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
        BuildProbabilityField, BuildProbabilityQuery, FinesseMetric, FinessePatternKnowledge,
        FinessePlacement, FinesseScoreRequest,
    };
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::invalid_query_reason;

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
}
