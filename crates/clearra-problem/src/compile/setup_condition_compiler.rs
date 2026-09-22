use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_pc_graph::request::{
    PcCountPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow, SupplyWindowSize,
};
use clearra_supply::queue::queue_pattern_expression::{
    QueuePatternExpression, QueuePatternParseError,
};

use crate::{
    compile::{ProblemCompileError, ProblemCompiler},
    query::{SetupCycleResetBorrowPolicy, SetupHoldPolicy, SetupSearchMode, SetupSearchQuery},
    SearchProblem,
};

#[derive(Clone, Debug)]
pub struct SetupSearchCondition {
    condition_id: String,
    cycle: u8,
    initial_hold: Option<PieceKind>,
    queue_remainder: Vec<PieceKind>,
    queue_based_expression: Option<QueuePatternExpression>,
    queue_based_start: usize,
    probability_pattern_count: usize,
    terminal_supply_target: Option<SetupTerminalSupplyTarget>,
    max_patterns: usize,
    pattern_expression: String,
    problem: SearchProblem,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SetupTerminalSupplyTarget {
    counts: [u8; 7],
    first_bag_boundary: u8,
}

impl SetupTerminalSupplyTarget {
    pub const fn counts(self) -> [u8; 7] {
        self.counts
    }

    pub const fn first_bag_boundary(self) -> u8 {
        self.first_bag_boundary
    }
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

    pub fn queue_based_expression(&self) -> Option<&QueuePatternExpression> {
        self.queue_based_expression.as_ref()
    }

    pub const fn queue_based_start(&self) -> usize {
        self.queue_based_start
    }

    /// Conditional probability denominator. QB observations renormalize the
    /// compatible supply language; terminal next-cycle filtering deliberately
    /// does not alter this count.
    pub const fn probability_pattern_count(&self) -> usize {
        self.probability_pattern_count
    }

    pub const fn terminal_supply_target(&self) -> Option<SetupTerminalSupplyTarget> {
        self.terminal_supply_target
    }

    pub const fn max_patterns(&self) -> usize {
        self.max_patterns
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
    InvalidRemainingPieceMultiplicity,
    QueueRemainderDuplicatePiece,
    PostCycleBorrowOutsideCycleSeven,
    QueueBasedQueueOrPatternRequired,
    QueueBasedPieceCountInvalid,
    QueueBasedObservedPieceCountInvalid,
    QueueBasedPatternLimitExceeded,
    NextCycleRemainingPieceCountInvalid,
    NextCycleRemainingDuplicatePiece,
    InitialHoldPieceMissing,
    Pattern(QueuePatternParseError),
    Problem(ProblemCompileError),
}

pub fn compile_setup_search_conditions(
    query: &SetupSearchQuery,
) -> Result<Vec<SetupSearchCondition>, SetupConditionCompileError> {
    let (cycle, pieces, hold_conditions) = setup_condition_inputs(query)?;
    let queue_based_expression = queue_based_expression(query)?;
    let terminal_supply_target = next_cycle_remaining_target(query, cycle)?;
    let mut conditions = hold_conditions
        .into_iter()
        .map(|initial_hold| {
            compile_condition(
                query,
                cycle,
                &pieces,
                queue_based_expression.as_ref(),
                terminal_supply_target,
                initial_hold,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(detail) = query.path_detail() {
        conditions.retain(|condition| condition.condition_id() == detail.condition_id());
    }
    Ok(conditions)
}

pub fn setup_search_condition_count(
    query: &SetupSearchQuery,
) -> Result<usize, SetupConditionCompileError> {
    let (_, _, hold_conditions) = setup_condition_inputs(query)?;
    Ok(selected_hold_conditions(query, hold_conditions).count())
}

pub fn compile_setup_search_condition(
    query: &SetupSearchQuery,
    condition_index: usize,
) -> Result<Option<SetupSearchCondition>, SetupConditionCompileError> {
    let (cycle, pieces, hold_conditions) = setup_condition_inputs(query)?;
    let queue_based_expression = queue_based_expression(query)?;
    let terminal_supply_target = next_cycle_remaining_target(query, cycle)?;
    selected_hold_conditions(query, hold_conditions)
        .nth(condition_index)
        .map(|initial_hold| {
            compile_condition(
                query,
                cycle,
                &pieces,
                queue_based_expression.as_ref(),
                terminal_supply_target,
                initial_hold,
            )
        })
        .transpose()
}

fn selected_hold_conditions(
    query: &SetupSearchQuery,
    hold_conditions: Vec<Option<PieceKind>>,
) -> impl Iterator<Item = Option<PieceKind>> + '_ {
    hold_conditions.into_iter().filter(|initial_hold| {
        query
            .path_detail()
            .is_none_or(|detail| condition_id(*initial_hold).as_str() == detail.condition_id())
    })
}

fn queue_based_expression(
    query: &SetupSearchQuery,
) -> Result<Option<QueuePatternExpression>, SetupConditionCompileError> {
    match query.search_mode() {
        SetupSearchMode::ShapeOracle => Ok(None),
        SetupSearchMode::QueueBased => queue_based_pattern(query).map(Some),
    }
}

fn queue_based_pattern(
    query: &SetupSearchQuery,
) -> Result<QueuePatternExpression, SetupConditionCompileError> {
    let expression = if let Some(expression) = query.queue().as_pattern_expression() {
        expression.clone()
    } else if let Some(sequence) = query.queue().as_fixed_sequence() {
        let source = sequence
            .pieces()
            .iter()
            .map(|piece| piece.as_ascii())
            .collect::<String>();
        QueuePatternExpression::parse(&source, query.limits().max_patterns())
            .map_err(SetupConditionCompileError::Pattern)?
    } else {
        return Err(SetupConditionCompileError::QueueBasedQueueOrPatternRequired);
    };
    if expression.sequence_len() == 0 {
        return Err(SetupConditionCompileError::QueueBasedPieceCountInvalid);
    }
    if expression.sequence_len() + query.residue().remaining_count() > 11 {
        return Err(SetupConditionCompileError::QueueBasedObservedPieceCountInvalid);
    }
    if expression.pattern_count() > query.limits().max_patterns() {
        return Err(SetupConditionCompileError::QueueBasedPatternLimitExceeded);
    }
    Ok(expression)
}

fn next_cycle_remaining_target(
    query: &SetupSearchQuery,
    cycle: u8,
) -> Result<Option<[u8; 7]>, SetupConditionCompileError> {
    let Some(pieces) = query.next_cycle_remaining_pieces() else {
        return Ok(None);
    };
    if pieces.len() != next_cycle_remaining_count(cycle) {
        return Err(SetupConditionCompileError::NextCycleRemainingPieceCountInvalid);
    }
    let mut counts = [0_u8; 7];
    for piece in pieces {
        counts[piece_index(*piece)] += 1;
    }
    if counts.iter().any(|count| *count > 2)
        || counts.iter().filter(|count| **count == 2).count() > 1
    {
        return Err(SetupConditionCompileError::NextCycleRemainingDuplicatePiece);
    }
    Ok(Some(counts))
}

const fn next_cycle_remaining_count(cycle: u8) -> usize {
    match cycle {
        1 => 4,
        2 => 1,
        3 => 5,
        4 => 2,
        5 => 6,
        6 => 3,
        7 => 7,
        _ => 0,
    }
}

const fn piece_index(piece: PieceKind) -> usize {
    match piece {
        PieceKind::I => 0,
        PieceKind::O => 1,
        PieceKind::T => 2,
        PieceKind::S => 3,
        PieceKind::Z => 4,
        PieceKind::J => 5,
        PieceKind::L => 6,
    }
}

type SetupConditionInputs = (u8, Vec<PieceKind>, Vec<Option<PieceKind>>);

fn setup_condition_inputs(
    query: &SetupSearchQuery,
) -> Result<SetupConditionInputs, SetupConditionCompileError> {
    let cycle = crate::query::setup_residue_input::cycle_for_remaining_count(
        query.residue().remaining_count(),
    )
    .ok_or(SetupConditionCompileError::InvalidRemainingPieceCount)?;
    if query.cycle_reset_borrow_policy() == SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse
        && cycle != 7
    {
        return Err(SetupConditionCompileError::PostCycleBorrowOutsideCycleSeven);
    }

    if !query.residue().has_valid_piece_multiplicity() {
        return Err(SetupConditionCompileError::InvalidRemainingPieceMultiplicity);
    }
    let mut pieces = canonical_pieces(query.residue().pieces());
    let automatic_initial_hold = query.residue().duplicate_piece();
    let initial_hold = match query.hold_policy() {
        SetupHoldPolicy::Disabled => None,
        SetupHoldPolicy::EnabledEmpty => automatic_initial_hold,
        SetupHoldPolicy::EnabledWithPiece(piece) => Some(piece),
    };
    if let Some(hold) = initial_hold {
        let index = pieces
            .iter()
            .position(|piece| *piece == hold)
            .ok_or(SetupConditionCompileError::InitialHoldPieceMissing)?;
        pieces.remove(index);
    }
    if PieceKind::STANDARD_TETROMINOES
        .into_iter()
        .any(|piece| pieces.iter().filter(|value| **value == piece).count() > 1)
    {
        return Err(SetupConditionCompileError::QueueRemainderDuplicatePiece);
    }
    Ok((cycle, pieces, vec![initial_hold]))
}

fn compile_condition(
    query: &SetupSearchQuery,
    cycle: u8,
    pieces: &[PieceKind],
    queue_based_expression: Option<&QueuePatternExpression>,
    terminal_supply_counts: Option<[u8; 7]>,
    initial_hold: Option<PieceKind>,
) -> Result<SetupSearchCondition, SetupConditionCompileError> {
    let queue_remainder = pieces.to_vec();
    let required_supply_len = queue_based_expression.map_or(0, |expression| {
        pieces.len().saturating_add(expression.sequence_len())
    });
    let pattern_expression = pattern_expression(
        &queue_remainder,
        query.residue().remaining_count(),
        query.cycle_reset_borrow_policy(),
        required_supply_len,
    );
    // Terminal inventory filtering is applied to the exact terminal supply state.
    // Keep the broad source factorized here so the compatible subset can retain
    // the original probability denominator.
    let expression = QueuePatternExpression::parse(
        &pattern_expression,
        if terminal_supply_counts.is_some() || queue_based_expression.is_some() {
            0
        } else {
            query.limits().max_patterns()
        },
    )
    .map_err(SetupConditionCompileError::Pattern)?;
    let sequence_len = expression.sequence_len();
    let probability_pattern_count =
        queue_based_expression.map_or(expression.pattern_count(), |qb| {
            conditioned_pattern_count(queue_remainder.len(), sequence_len, qb).unwrap_or(usize::MAX)
        });
    if probability_pattern_count == 0 || probability_pattern_count > query.limits().max_patterns() {
        return Err(SetupConditionCompileError::QueueBasedPatternLimitExceeded);
    }
    let scenario = PcScenarioQuery::new(
        PcScenarioBoard::standard_10(4, 0),
        PcQueueInput::pattern_expression(expression),
        PieceWindow::new(10),
    )
    .with_rule(query.rule())
    .with_queue_observation_policy(query.queue_observation_policy())
    .with_hold_piece(initial_hold)
    .with_allow_hold(query.hold_policy().is_enabled())
    .with_exact_pieces(Some(10))
    .with_supply_window_size(SupplyWindowSize::new(sequence_len))
    .with_count_policy(PcCountPolicy::CountUnique)
    .with_execution_policy(
        clearra_pc_graph::request::PcExecutionPolicy::mvp_default()
            .with_exact_legal_board_enabled(query.exact_legal_board_enabled())
            .with_conditioned_reachability_enabled(query.conditioned_reachability_enabled()),
    )
    .with_retained_trace_limit(query.limits().post_pc_retained_trace_limit());
    let problem = ProblemCompiler::compile_scenario_pc(&scenario)
        .map_err(SetupConditionCompileError::Problem)?;
    let condition_id = condition_id(initial_hold);

    Ok(SetupSearchCondition {
        condition_id,
        cycle,
        initial_hold,
        queue_remainder,
        queue_based_expression: queue_based_expression.cloned(),
        queue_based_start: pieces.len(),
        probability_pattern_count,
        terminal_supply_target: terminal_supply_counts.map(|counts| SetupTerminalSupplyTarget {
            counts,
            first_bag_boundary: pieces.len() as u8,
        }),
        max_patterns: query.limits().max_patterns(),
        pattern_expression,
        problem,
    })
}

fn condition_id(initial_hold: Option<PieceKind>) -> String {
    initial_hold.map_or_else(
        || "hold-empty".to_owned(),
        |piece| format!("hold-{}", piece.as_ascii()),
    )
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
    required_supply_len: usize,
) -> String {
    // Before cycle seven, the next bag still belongs to the same PC window.
    // Materialize one additional draw so Hold may leave the final queue piece
    // unplaced instead of forcing the held piece to be the leftover.
    // Cycle seven deliberately stops at the reset boundary unless the caller
    // explicitly permits borrowing from the following cycle.
    let materialized_hold_slack = remaining_count != 3
        || borrow_policy == SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse;
    let materialized_supply_len =
        (10 + usize::from(materialized_hold_slack)).max(required_supply_len);
    let future_draws = materialized_supply_len.saturating_sub(remaining_count);
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

fn conditioned_pattern_count(
    queue_remainder_len: usize,
    sequence_len: usize,
    qb: &QueuePatternExpression,
) -> Option<usize> {
    let suffix_len = sequence_len.checked_sub(queue_remainder_len)?;
    if qb.sequence_len() > suffix_len {
        return None;
    }
    let current_orders = falling_factorial(queue_remainder_len, queue_remainder_len)?;
    let continuation_orders =
        standard_bag_continuation_count(qb.sequence_len(), suffix_len - qb.sequence_len())?;
    current_orders
        .checked_mul(qb.pattern_count())?
        .checked_mul(continuation_orders)
}

fn standard_bag_continuation_count(prefix_len: usize, mut draws: usize) -> Option<usize> {
    let mut available = if prefix_len % 7 == 0 {
        7
    } else {
        7 - prefix_len % 7
    };
    let mut count = 1_usize;
    while draws != 0 {
        let take = draws.min(available);
        count = count.checked_mul(falling_factorial(available, take)?)?;
        draws -= take;
        available -= take;
        if available == 0 {
            available = 7;
        }
    }
    Some(count)
}

fn falling_factorial(value: usize, count: usize) -> Option<usize> {
    (0..count).try_fold(1_usize, |product, offset| {
        product.checked_mul(value.checked_sub(offset)?)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_rules::profile::builtin_rules::srs_x;

    #[test]
    fn unique_residue_defaults_to_one_empty_hold_condition() {
        let query = SetupSearchQuery::default().with_remaining_pieces(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
        ]);

        let conditions = compile_setup_search_conditions(&query).expect("conditions");

        assert_eq!(conditions.len(), 1);
        assert_eq!(conditions[0].condition_id(), "hold-empty");
        assert_eq!(conditions[0].pattern_expression(), "[IOT]!P7");
        assert_eq!(conditions[0].queue_remainder().len(), 3);
    }

    #[test]
    fn cycle_one_compiles_the_complete_factorized_pattern_domain() {
        let query = SetupSearchQuery::default();

        let conditions = compile_setup_search_conditions(&query).expect("cycle one condition");

        assert_eq!(conditions.len(), 1);
        assert_eq!(conditions[0].condition_id(), "hold-empty");
        assert_eq!(conditions[0].pattern_expression(), "[IOTSZJL]!P4");
        let universe = conditions[0]
            .problem()
            .piece_source()
            .materialized_universe()
            .expect("factorized pattern universe");
        assert_eq!(universe.pattern_count(), 4_233_600);
        assert!(matches!(
            universe.structure(),
            clearra_supply::pattern_universe::MaterializedPatternUniverseStructure::FactorizedQueueExpression { .. }
        ));
    }

    #[test]
    fn selected_kick_table_reaches_every_compiled_setup_condition() {
        let query = SetupSearchQuery::default()
            .with_rule(srs_x())
            .with_remaining_pieces(vec![PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::S]);

        let conditions = compile_setup_search_conditions(&query).expect("conditions");

        assert!(!conditions.is_empty());
        assert!(conditions
            .iter()
            .all(|condition| condition.problem().rule_profile_value() == srs_x()));
    }

    #[test]
    fn explicit_initial_hold_compiles_only_the_selected_condition() {
        let query = SetupSearchQuery::default()
            .with_remaining_pieces(vec![PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::S])
            .with_hold_policy(SetupHoldPolicy::EnabledWithPiece(PieceKind::S));

        let conditions = compile_setup_search_conditions(&query).expect("conditions");

        assert_eq!(conditions.len(), 1);
        assert_eq!(conditions[0].condition_id(), "hold-S");
        assert_eq!(conditions[0].initial_hold(), Some(PieceKind::S));
        assert_eq!(
            conditions[0].queue_remainder(),
            &[PieceKind::I, PieceKind::O, PieceKind::T]
        );
        assert_eq!(conditions[0].pattern_expression(), "[IOT]!P7");
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
    fn one_duplicate_residue_piece_becomes_the_automatic_initial_hold() {
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
        assert_eq!(conditions[0].condition_id(), "hold-S");
        assert_eq!(
            conditions[0].queue_remainder(),
            &[PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::S]
        );
        assert_eq!(conditions[0].pattern_expression(), "[IOTS]!P6");
        assert_eq!(conditions[0].cycle(), 4);
    }

    #[test]
    fn explicit_initial_hold_may_match_one_queue_remainder_piece() {
        let query = SetupSearchQuery::default()
            .with_remaining_pieces(vec![
                PieceKind::I,
                PieceKind::O,
                PieceKind::T,
                PieceKind::S,
                PieceKind::S,
            ])
            .with_hold_policy(SetupHoldPolicy::EnabledWithPiece(PieceKind::S));

        let conditions = compile_setup_search_conditions(&query).expect("condition");

        assert_eq!(conditions.len(), 1);
        assert_eq!(conditions[0].initial_hold(), Some(PieceKind::S));
        assert_eq!(conditions[0].pattern_expression(), "[IOTS]!P6");
        assert_eq!(conditions[0].cycle(), 4);
    }

    #[test]
    fn automatic_initial_hold_rejects_two_duplicated_piece_kinds() {
        let query = SetupSearchQuery::default().with_remaining_pieces(vec![
            PieceKind::I,
            PieceKind::I,
            PieceKind::O,
            PieceKind::O,
        ]);

        assert!(matches!(
            compile_setup_search_conditions(&query),
            Err(SetupConditionCompileError::InvalidRemainingPieceMultiplicity)
        ));
    }

    #[test]
    fn cycle_three_and_five_cross_two_future_bags() {
        assert_eq!(
            pattern_expression(
                &[PieceKind::T],
                1,
                SetupCycleResetBorrowPolicy::default(),
                0,
            ),
            "TP7P3"
        );
        assert_eq!(
            // The post-setup continuation retains this cross-bag source; it is
            // not replaced by a fresh P7 when a partial setup is selected.
            pattern_expression(
                &[PieceKind::S, PieceKind::Z],
                2,
                SetupCycleResetBorrowPolicy::default(),
                0,
            ),
            "[SZ]!P7P2"
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

        assert_eq!(conditions.len(), 1);
        assert_eq!(conditions[0].pattern_expression(), "[IOTS]!P7");
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
    fn path_detail_compiles_only_the_requested_hold_condition() {
        let detail = crate::query::SetupPathDetail::new(1, 0, 1, "hold-T").expect("path detail");
        let query = SetupSearchQuery::default()
            .with_remaining_pieces(vec![PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::S])
            .with_hold_policy(SetupHoldPolicy::EnabledWithPiece(PieceKind::T))
            .with_path_detail(detail);

        assert_eq!(
            setup_search_condition_count(&query).expect("condition count"),
            1
        );
        let condition = compile_setup_search_condition(&query, 0)
            .expect("condition compile")
            .expect("selected condition");
        assert_eq!(condition.condition_id(), "hold-T");
        assert_eq!(condition.initial_hold(), Some(PieceKind::T));
        assert!(compile_setup_search_condition(&query, 1)
            .expect("out of range")
            .is_none());
        assert_eq!(
            compile_setup_search_conditions(&query)
                .expect("eager conditions")
                .len(),
            1
        );
    }

    #[test]
    fn cycle_seven_borrow_adds_one_post_reset_draw_only_when_requested() {
        let remainder = [PieceKind::T, PieceKind::S, PieceKind::Z];
        assert_eq!(
            pattern_expression(&remainder, 3, SetupCycleResetBorrowPolicy::default(), 0),
            "[TSZ]!P7"
        );
        assert_eq!(
            pattern_expression(
                &remainder,
                3,
                SetupCycleResetBorrowPolicy::AllowPostCyclePieceUse,
                0,
            ),
            "[TSZ]!P7P1"
        );
    }

    #[test]
    fn queue_based_condition_keeps_the_canonical_bag_universe_for_later_intersection() {
        let query = SetupSearchQuery::default()
            .with_remaining_pieces(vec![PieceKind::I, PieceKind::I, PieceKind::O, PieceKind::T])
            .with_hold_policy(SetupHoldPolicy::EnabledWithPiece(PieceKind::I))
            .with_queue_based_pieces(vec![PieceKind::S]);

        let conditions = compile_setup_search_conditions(&query).expect("QB condition");

        assert_eq!(conditions.len(), 1);
        assert_eq!(conditions[0].condition_id(), "hold-I");
        assert_eq!(conditions[0].initial_hold(), Some(PieceKind::I));
        assert_eq!(
            conditions[0].queue_remainder(),
            &[PieceKind::I, PieceKind::O, PieceKind::T]
        );
        assert_eq!(conditions[0].pattern_expression(), "[IOT]!P7");
        assert_eq!(conditions[0].queue_based_start(), 3);
        assert_eq!(conditions[0].probability_pattern_count(), 4_320);
        assert!(conditions[0].terminal_supply_target().is_none());
    }

    #[test]
    fn exact_os_is_an_ordered_next_bag_prefix_not_an_unordered_group() {
        let query = SetupSearchQuery::default()
            .with_remaining_pieces(vec![PieceKind::T, PieceKind::I])
            .with_queue_based_pieces(vec![PieceKind::O, PieceKind::S]);

        let condition = compile_setup_search_conditions(&query)
            .expect("exact QB condition")
            .remove(0);
        let qb = condition.queue_based_expression().expect("QB expression");

        assert_eq!(condition.pattern_expression(), "[IT]!P7P2");
        assert_eq!(condition.queue_based_start(), 2);
        assert_eq!(condition.probability_pattern_count(), 10_080);
        assert!(qb.matches_sequence(&[PieceKind::O, PieceKind::S]));
        assert!(!qb.matches_sequence(&[PieceKind::S, PieceKind::O]));
    }

    #[test]
    fn qb_group_pattern_explicitly_allows_both_os_orders() {
        let qb = QueuePatternExpression::parse("[OS]!", 10).expect("QB pattern");
        let query = SetupSearchQuery::default()
            .with_remaining_pieces(vec![PieceKind::T, PieceKind::I])
            .with_queue_based_pattern_expression(qb);

        let condition = compile_setup_search_conditions(&query)
            .expect("pattern QB condition")
            .remove(0);
        let qb = condition.queue_based_expression().expect("QB expression");

        assert_eq!(condition.probability_pattern_count(), 20_160);
        assert!(qb.matches_sequence(&[PieceKind::O, PieceKind::S]));
        assert!(qb.matches_sequence(&[PieceKind::S, PieceKind::O]));
    }

    #[test]
    fn explicit_initial_hold_must_be_present_in_the_supplied_inventory() {
        let query = SetupSearchQuery::default()
            .with_remaining_pieces(vec![PieceKind::I, PieceKind::O, PieceKind::T])
            .with_hold_policy(SetupHoldPolicy::EnabledWithPiece(PieceKind::S));

        assert!(matches!(
            compile_setup_search_conditions(&query),
            Err(SetupConditionCompileError::InitialHoldPieceMissing)
        ));
    }

    #[test]
    fn observed_queue_and_next_cycle_inventory_compile_independently() {
        let query = SetupSearchQuery::default()
            .with_remaining_pieces(vec![PieceKind::T, PieceKind::I])
            .with_queue_based_pieces(vec![PieceKind::O, PieceKind::S])
            .with_next_cycle_remaining_pieces(vec![
                PieceKind::O,
                PieceKind::O,
                PieceKind::S,
                PieceKind::I,
                PieceKind::T,
                PieceKind::Z,
            ]);

        let conditions = compile_setup_search_conditions(&query).expect("QB condition");

        assert_eq!(conditions.len(), 1);
        assert_eq!(conditions[0].pattern_expression(), "[IT]!P7P2");
        assert_eq!(
            conditions[0]
                .terminal_supply_target()
                .expect("terminal target")
                .counts(),
            [1, 2, 1, 1, 1, 0, 0]
        );
    }

    #[test]
    fn queue_based_condition_rejects_too_many_observed_pieces() {
        let query = SetupSearchQuery::default()
            .with_remaining_pieces(vec![PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::S])
            .with_queue_based_pieces(vec![
                PieceKind::I,
                PieceKind::O,
                PieceKind::T,
                PieceKind::S,
                PieceKind::Z,
                PieceKind::J,
                PieceKind::L,
                PieceKind::I,
            ]);

        assert!(matches!(
            compile_setup_search_conditions(&query),
            Err(SetupConditionCompileError::QueueBasedObservedPieceCountInvalid)
        ));
    }

    #[test]
    fn queue_based_condition_accepts_the_full_eleven_piece_input_window() {
        let query = SetupSearchQuery::default()
            .with_remaining_pieces(vec![PieceKind::I])
            .with_queue_based_pieces(vec![
                PieceKind::I,
                PieceKind::O,
                PieceKind::T,
                PieceKind::S,
                PieceKind::Z,
                PieceKind::J,
                PieceKind::L,
                PieceKind::I,
                PieceKind::O,
                PieceKind::T,
            ]);

        let condition = compile_setup_search_conditions(&query)
            .expect("eleven-piece combined input")
            .remove(0);
        assert_eq!(condition.queue_based_start(), 1);
        assert_eq!(condition.pattern_expression(), "IP7P3");
        assert_eq!(
            condition
                .queue_based_expression()
                .expect("QB expression")
                .sequence_len(),
            10
        );
    }

    #[test]
    fn next_cycle_inventory_is_available_in_oracle_mode() {
        let query = SetupSearchQuery::default()
            .with_remaining_pieces(vec![PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::S])
            .with_next_cycle_remaining_pieces(vec![PieceKind::Z]);

        let conditions = compile_setup_search_conditions(&query).expect("oracle condition");

        assert_eq!(query.search_mode(), SetupSearchMode::ShapeOracle);
        assert_eq!(conditions[0].pattern_expression(), "[IOTS]!P7");
        assert_eq!(
            conditions[0]
                .terminal_supply_target()
                .expect("terminal target")
                .counts(),
            [0, 0, 0, 0, 1, 0, 0]
        );
    }

    #[test]
    fn next_cycle_inventory_requires_the_cycle_specific_piece_count() {
        let query = SetupSearchQuery::default()
            .with_remaining_pieces(vec![PieceKind::I, PieceKind::O, PieceKind::T, PieceKind::S])
            .with_next_cycle_remaining_pieces(vec![PieceKind::Z, PieceKind::J]);

        assert!(matches!(
            compile_setup_search_conditions(&query),
            Err(SetupConditionCompileError::NextCycleRemainingPieceCountInvalid)
        ));
    }

    #[test]
    fn next_cycle_inventory_rejects_multiple_duplicate_kinds() {
        let query = SetupSearchQuery::default()
            .with_remaining_pieces(vec![PieceKind::T, PieceKind::I])
            .with_next_cycle_remaining_pieces(vec![
                PieceKind::O,
                PieceKind::O,
                PieceKind::T,
                PieceKind::T,
                PieceKind::S,
                PieceKind::Z,
            ]);

        assert!(matches!(
            compile_setup_search_conditions(&query),
            Err(SetupConditionCompileError::NextCycleRemainingDuplicatePiece)
        ));
    }
}
