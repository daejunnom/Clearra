use clearra_app::{AppCommand, ScenarioAppCommand};
use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_pc_graph::request::{
    PcCountPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PcSolutionProbabilityPolicy,
    PieceWindow,
};
use clearra_supply::queue::queue_pattern_expression::QueuePatternExpression;

use crate::{
    model::{GuiBackendForm, GuiScenarioPcForm},
    request::{
        execution_constraint_objective_policy, parse_piece_sequence, parse_queue_pattern,
        parse_rule_profile, score_objective_policy, BackendRequestBuilder, RequestBuildError,
        RequestBuildErrorCode,
    },
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScenarioRequestBuilder;

impl ScenarioRequestBuilder {
    pub fn build_command(
        form: &GuiScenarioPcForm,
        backend: &GuiBackendForm,
    ) -> Result<AppCommand, RequestBuildError> {
        if form.visible_height() == 0 {
            return Err(RequestBuildError::new(
                RequestBuildErrorCode::InvalidLineCount,
                "GUI scenario form requires a positive visible height",
            ));
        }
        let standard_bag_pattern = form
            .remaining_queue_is_pattern()
            .then(|| {
                QueuePatternExpression::standard_7_bag_with_optional_leading_piece(
                    form.remaining_queue(),
                )
            })
            .flatten();
        let leading_supply_piece = standard_bag_pattern
            .and_then(|(leading_piece, _)| leading_piece)
            .filter(|_| form.allow_hold() && form.hold_piece().is_none());
        let finite_standard_bag_len = standard_bag_pattern.and_then(|(leading_piece, length)| {
            (leading_piece.is_none() || leading_supply_piece.is_some()).then_some(length)
        });
        let queue = if form.remaining_queue_is_standard_bag() || finite_standard_bag_len.is_some() {
            PcQueueInput::standard_7_bag()
        } else if form.remaining_queue_is_pattern() {
            PcQueueInput::pattern_expression(parse_queue_pattern(
                form.remaining_queue(),
                backend.pattern_budget() as usize,
                "scenario queue pattern",
            )?)
        } else {
            PcQueueInput::fixed_sequence(parse_piece_sequence(
                form.remaining_queue(),
                "scenario remaining queue",
            )?)
        };
        if !form.allow_hold() && form.hold_piece().is_some() {
            return Err(RequestBuildError::new(
                RequestBuildErrorCode::ValidationFailed,
                "GUI scenario cannot use an occupied hold slot when hold is disabled",
            ));
        }
        let hold_piece = form
            .hold_piece()
            .map(PieceKind::from_ascii)
            .transpose()
            .map_err(|error| {
                RequestBuildError::new(
                    RequestBuildErrorCode::UnknownPiece,
                    format!("invalid GUI scenario hold piece: {error:?}"),
                )
            })?
            .or(leading_supply_piece);
        let mut count_policy = match form.count_policy() {
            "unique" | "count-unique" => PcCountPolicy::CountUnique,
            "all" | "count-all" => PcCountPolicy::CountAll,
            value => {
                return Err(RequestBuildError::new(
                    RequestBuildErrorCode::ValidationFailed,
                    format!("invalid GUI scenario count policy '{value}'"),
                ))
            }
        };
        let base_objective = match count_policy {
            PcCountPolicy::CountAll => {
                clearra_objectives::policy::objective_policy::ObjectivePolicy::all()
            }
            PcCountPolicy::FirstSolution | PcCountPolicy::CountUnique => {
                clearra_objectives::policy::objective_policy::ObjectivePolicy::unique()
            }
        };
        let objective = score_objective_policy(
            form.score_mode(),
            form.score_profile(),
            form.spin_profile(),
            form.initial_b2b(),
            base_objective,
        )?;
        let objective = execution_constraint_objective_policy(
            form.preserve_b2b(),
            form.spin_profile(),
            objective,
        )?;
        if objective.score().requested() {
            count_policy = PcCountPolicy::CountAll;
        }
        let mut query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(
                u16::from(form.visible_height()),
                form.initial_board_mask(),
            ),
            queue,
            PieceWindow::new(form.piece_window()),
        )
        .with_rule(parse_rule_profile(form.rule())?)
        .with_queue_observation_policy(form.queue_observation_policy())
        .with_hold_piece(hold_piece)
        .with_allow_hold(form.allow_hold())
        .with_count_policy(count_policy)
        .with_objective(objective)
        .with_execution_policy(BackendRequestBuilder::build_execution_policy(backend)?);
        if let Some(length) = finite_standard_bag_len {
            let automatic_source_pieces = form
                .piece_window()
                .saturating_add(usize::from(form.allow_hold()))
                .saturating_sub(usize::from(hold_piece.is_some()));
            query =
                query.with_supply_window_size(clearra_pc_graph::request::SupplyWindowSize::new(
                    length.min(automatic_source_pieces),
                ));
        }
        if form.solution_probabilities() {
            query = query.with_solution_probability_policy(PcSolutionProbabilityPolicy::Include);
        }

        Ok(AppCommand::Scenario(ScenarioAppCommand::new(query)))
    }
}
