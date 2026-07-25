use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_pc_graph::request::{
    PcCountPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow, SupplyWindowSize,
};
use clearra_supply::queue::queue_pattern_expression::{
    QueuePatternExpression, QueuePatternParseError,
};

use crate::{
    compile::{ProblemCompileError, ProblemCompiler},
    query::{SetupCycleResetBorrowPolicy, SetupSearchQuery},
    SearchProblem,
};

#[derive(Clone, Debug)]
pub struct SetupSearchCondition {
    condition_id: String,
    cycle: u8,
    initial_hold: Option<PieceKind>,
    queue_remainder: Vec<PieceKind>,
    pattern_expression: String,
    problem: SearchProblem,
}

impl SetupSearchCondition {
    pub fn condition_id(&self) -> &str {
        &self.condition_id
    }

    pub fn cycle(&self) -> u8 {
        self.cycle
    }

    pub fn initial_hold(&self) -> Option<PieceKind> {
        self.initial_hold
    }

    pub fn queue_remainder(&self) -> &[PieceKind] {
        &self.queue_remainder
    }

    pub fn pattern_expression(&self) -> &str {
        &self.pattern_expression
    }

    pub fn problem(&self) -> &SearchProblem {
        &self.problem
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SetupConditionCompileError {
    InvalidRemainingPieceCount,
    MultipleDuplicateKinds,
    PieceOccursMoreThanTwice,
    PostCycleBorrowOutsideCycleSeven,
    Pattern(QueuePatternParseError),
    Problem(ProblemCompileError),
}

pub fn compile_setup_search_conditions(
    query: &SetupSearchQuery,
) -> Result<Vec<SetupSearchCondition>, SetupConditionCompileError> {
    let (cycle, pieces, hold_conditions) = setup_condition_inputs(query)?;
    hold_conditions
        .into_iter()
        .map(|initial_hold| compile_condition(query, cycle, &pieces, initial_hold))
        .collect()
}

pub fn setup_search_condition_count(
    query: &SetupSearchQuery,
) -> Result<usize, SetupConditionCompileError> {
    Ok(setup_condition_inputs(query)?.2.len())
}

pub fn compile_setup_search_condition(
    query: &SetupSearchQuery,
    condition_index: usize,
) -> Result<Option<SetupSearchCondition>, SetupConditionCompileError> {
    let (cycle, pieces, hold_conditions) = setup_condition_inputs(query)?;
    hold_conditions
        .get(condition_index)
        .copied()
        .map(|initial_hold| compile_condition(query, cycle, &pieces, initial_hold))
        .transpose()
}

fn setup_condition_inputs(
    query: &SetupSearchQuery,
) -> Result<(u8, Vec<PieceKind>, Vec<Option<PieceKind>>), SetupConditionCompileError> {
    let cycle = query
        .residue()
        .cycle()
        .ok_or(SetupConditionCompileError::InvalidRemainingPieceCount)?;
    if query.cycle_reset_borrow_policy() == SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse
        && cycle != 7
    {
        return Err(SetupConditionCompileError::PostCycleBorrowOutsideCycleSeven);
    }

    let pieces = canonical_pieces(query.residue().pieces());
    let duplicate_kinds = PieceKind::STANDARD_TETROMINOES
        .into_iter()
        .filter(|piece| pieces.iter().filter(|value| **value == *piece).count() > 1)
        .collect::<Vec<_>>();
    if duplicate_kinds.len() > 1 {
        return Err(SetupConditionCompileError::MultipleDuplicateKinds);
    }
    if pieces.iter().any(|piece| {
        pieces
            .iter()
            .filter(|candidate| *candidate == piece)
            .count()
            > 2
    }) {
        return Err(SetupConditionCompileError::PieceOccursMoreThanTwice);
    }

    let hold_conditions = if let Some(duplicate) = duplicate_kinds.first().copied() {
        vec![Some(duplicate)]
    } else {
        let mut conditions = Vec::with_capacity(pieces.len() + 1);
        conditions.push(None);
        conditions.extend(pieces.iter().copied().map(Some));
        conditions
    };
    Ok((cycle, pieces, hold_conditions))
}

fn compile_condition(
    query: &SetupSearchQuery,
    cycle: u8,
    pieces: &[PieceKind],
    initial_hold: Option<PieceKind>,
) -> Result<SetupSearchCondition, SetupConditionCompileError> {
    let mut queue_remainder = pieces.to_vec();
    if let Some(hold) = initial_hold {
        let index = queue_remainder
            .iter()
            .position(|piece| *piece == hold)
            .expect("hold condition comes from the residue multiset");
        queue_remainder.remove(index);
    }
    let pattern_expression = pattern_expression(
        &queue_remainder,
        pieces.len(),
        query.cycle_reset_borrow_policy(),
    );
    let expression =
        QueuePatternExpression::parse(&pattern_expression, query.limits().max_patterns())
            .map_err(SetupConditionCompileError::Pattern)?;
    let sequence_len = expression.sequence_len();
    let scenario = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::pattern_expression(expression),
        PieceWindow::new(10),
    )
    .with_hold_piece(initial_hold)
    .with_allow_hold(true)
    .with_exact_pieces(Some(10))
    .with_supply_window_size(SupplyWindowSize::new(sequence_len))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_retained_trace_limit(query.limits().post_pc_retained_trace_limit());
    let problem = ProblemCompiler::compile_scenario_pc(&scenario)
        .map_err(SetupConditionCompileError::Problem)?;
    let condition_id = initial_hold.map_or_else(
        || "hold-empty".to_owned(),
        |piece| format!("hold-{}", piece.as_ascii()),
    );

    Ok(SetupSearchCondition {
        condition_id,
        cycle,
        initial_hold,
        queue_remainder,
        pattern_expression,
        problem,
    })
}

fn canonical_pieces(pieces: &[PieceKind]) -> Vec<PieceKind> {
    PieceKind::STANDARD_TETROMINOES
        .into_iter()
        .flat_map(|piece| {
            std::iter::repeat_n(
                piece,
                pieces.iter().filter(|value| **value == piece).count(),
            )
        })
        .collect()
}

fn pattern_expression(
    queue_remainder: &[PieceKind],
    remaining_count: usize,
    borrow_policy: SetupCycleResetBorrowPolicy,
) -> String {
    let locks_after_residue = 10usize.saturating_sub(remaining_count);
    // Before cycle seven, the next bag still belongs to the same PC window.
    // Materialize one additional draw so Hold may leave any observed piece
    // unplaced instead of forcing the final held piece to be the leftover.
    // Cycle seven deliberately stops at the reset boundary unless the caller
    // explicitly permits borrowing from the following cycle.
    let materialized_hold_slack = remaining_count != 3
        || borrow_policy == SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse;
    let future_draws = locks_after_residue + usize::from(materialized_hold_slack);
    let mut expression = String::new();
    match queue_remainder {
        [] => {}
        [piece] => expression.push(piece.as_ascii()),
        pieces => {
            expression.push('[');
            expression.extend(pieces.iter().map(|piece| piece.as_ascii()));
            expression.push_str("]!");
        }
    }
    let mut remaining_draws = future_draws;
    while remaining_draws != 0 {
        let draw = remaining_draws.min(7);
        expression.push('P');
        expression.push_str(&draw.to_string());
        remaining_draws -= draw;
    }
    expression
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_residue_splits_hold_conditions_without_averaging_them() {
        let query = SetupSearchQuery::default().with_remaining_pieces(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
        ]);

        let conditions = compile_setup_search_conditions(&query).expect("conditions");

        assert_eq!(conditions.len(), 4);
        assert_eq!(conditions[0].condition_id(), "hold-empty");
        assert_eq!(conditions[0].pattern_expression(), "[IOT]!P7");
        assert_eq!(conditions[1].condition_id(), "hold-I");
        assert_eq!(conditions[1].pattern_expression(), "[OT]!P7");
        assert_eq!(conditions[1].queue_remainder().len(), 2);
    }

    #[test]
    fn lazy_condition_compile_matches_eager_condition_order() {
        let query = SetupSearchQuery::default().with_remaining_pieces(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
        ]);
        let eager = compile_setup_search_conditions(&query).expect("eager conditions");

        assert_eq!(
            setup_search_condition_count(&query).expect("condition count"),
            eager.len()
        );
        for (index, expected) in eager.iter().enumerate() {
            let actual = compile_setup_search_condition(&query, index)
                .expect("lazy condition")
                .expect("condition exists");
            assert_eq!(actual.condition_id(), expected.condition_id());
            assert_eq!(actual.initial_hold(), expected.initial_hold());
            assert_eq!(actual.pattern_expression(), expected.pattern_expression());
            assert_eq!(actual.queue_remainder(), expected.queue_remainder());
        }
        assert!(compile_setup_search_condition(&query, eager.len())
            .expect("out of range is not a compile failure")
            .is_none());
    }

    #[test]
    fn one_duplicate_is_the_explicit_initial_hold() {
        let query = SetupSearchQuery::default().with_remaining_pieces(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
            PieceKind::S,
        ]);

        let conditions = compile_setup_search_conditions(&query).expect("condition");

        assert_eq!(conditions.len(), 1);
        assert_eq!(conditions[0].initial_hold(), Some(PieceKind::S));
        assert_eq!(conditions[0].pattern_expression(), "[IOTS]!P6");
        assert_eq!(conditions[0].cycle(), 4);
    }

    #[test]
    fn cycle_three_and_five_cross_two_future_bags() {
        assert_eq!(
            pattern_expression(&[PieceKind::T], 1, SetupCycleResetBorrowPolicy::default()),
            "TP7P3"
        );
        assert_eq!(
            pattern_expression(
                &[PieceKind::I, PieceKind::O],
                2,
                SetupCycleResetBorrowPolicy::default()
            ),
            "[IO]!P7P2"
        );
    }

    #[test]
    fn cycle_two_materializes_the_hold_slack_piece_from_the_next_bag() {
        let query = SetupSearchQuery::default().with_remaining_pieces(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
        ]);

        let conditions = compile_setup_search_conditions(&query).expect("conditions");

        assert_eq!(conditions[0].pattern_expression(), "[IOTS]!P7");
        assert_eq!(conditions[1].pattern_expression(), "[OTS]!P7");
    }

    #[test]
    fn residue_input_order_does_not_change_the_compiled_pattern_domain() {
        let iots = SetupSearchQuery::default().with_remaining_pieces(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
        ]);
        let stoi = SetupSearchQuery::default().with_remaining_pieces(vec![
            PieceKind::S,
            PieceKind::T,
            PieceKind::O,
            PieceKind::I,
        ]);

        let compile_summary = |query: &SetupSearchQuery| {
            compile_setup_search_conditions(query)
                .expect("conditions")
                .into_iter()
                .map(|condition| {
                    (
                        condition.condition_id().to_owned(),
                        condition.initial_hold(),
                        condition.queue_remainder().to_vec(),
                        condition.pattern_expression().to_owned(),
                    )
                })
                .collect::<Vec<_>>()
        };

        assert_eq!(compile_summary(&iots), compile_summary(&stoi));
    }

    #[test]
    fn cycle_seven_borrow_adds_one_post_reset_draw_only_when_requested() {
        let remainder = [PieceKind::T, PieceKind::S, PieceKind::Z];
        assert_eq!(
            pattern_expression(&remainder, 3, SetupCycleResetBorrowPolicy::default()),
            "[TSZ]!P7"
        );
        assert_eq!(
            pattern_expression(
                &remainder,
                3,
                SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse
            ),
            "[TSZ]!P7P1"
        );
    }
}
