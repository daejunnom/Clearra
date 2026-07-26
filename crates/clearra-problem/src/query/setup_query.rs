pub use super::{
    setup_candidate_priority::SetupCandidatePriority,
    setup_grouping::GroupingMode,
    setup_hold_policy::SetupHoldPolicy,
    setup_length_preference::SetupLengthPreference,
    setup_limits::{SetupLimits, SetupLimitsError},
    setup_piece_budget::{PieceBudget, PieceBudgetError},
    setup_probability_filter::{SetupProbabilityFilter, SetupProbabilityFilterError},
    setup_queue_input::SetupQueueInput,
    setup_residue_input::{SetupCycleResetBorrowPolicy, SetupResidueInput},
    setup_search_mode::SetupSearchMode,
};

use clearra_core_domain::{board::board_size::BoardSize, pc::pc_target::PcTarget};
use clearra_rules::profile::{builtin_rules::srs_plus, rule_profile::RuleProfile};
use clearra_supply::queue::fixed_sequence::FixedSequence;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupPathDetail {
    board_mask: u64,
    condition_id: String,
}

impl SetupPathDetail {
    pub fn new(board_mask: u64, condition_id: impl Into<String>) -> Option<Self> {
        let condition_id = condition_id.into();
        (board_mask >> 40 == 0 && !condition_id.is_empty()).then_some(Self {
            board_mask,
            condition_id,
        })
    }

    pub const fn board_mask(&self) -> u64 {
        self.board_mask
    }

    pub fn condition_id(&self) -> &str {
        &self.condition_id
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SetupSearchQuery {
    board_size: BoardSize,
    target: PcTarget,
    rule: RuleProfile,
    queue: SetupQueueInput,
    hold_policy: SetupHoldPolicy,
    piece_budget: PieceBudget,
    probability_filter: SetupProbabilityFilter,
    grouping_mode: GroupingMode,
    limits: SetupLimits,
    residue: SetupResidueInput,
    cycle_reset_borrow_policy: SetupCycleResetBorrowPolicy,
    candidate_priority: SetupCandidatePriority,
    length_preference: SetupLengthPreference,
    max_setup_pieces: u8,
    search_mode: SetupSearchMode,
    path_detail: Option<SetupPathDetail>,
}

impl SetupSearchQuery {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        board_size: BoardSize,
        target: PcTarget,
        queue: SetupQueueInput,
        hold_policy: SetupHoldPolicy,
        piece_budget: PieceBudget,
        probability_filter: SetupProbabilityFilter,
        grouping_mode: GroupingMode,
        limits: SetupLimits,
    ) -> Self {
        Self {
            board_size,
            target,
            rule: srs_plus(),
            queue,
            hold_policy,
            piece_budget,
            probability_filter,
            grouping_mode,
            limits,
            residue: SetupResidueInput::default(),
            cycle_reset_borrow_policy: SetupCycleResetBorrowPolicy::default(),
            candidate_priority: SetupCandidatePriority::default(),
            length_preference: SetupLengthPreference::default(),
            max_setup_pieces: 9,
            search_mode: SetupSearchMode::default(),
            path_detail: None,
        }
    }
}
impl SetupSearchQuery {
    pub fn board_size(&self) -> BoardSize {
        self.board_size
    }
}
impl SetupSearchQuery {
    pub fn target(&self) -> PcTarget {
        self.target
    }
}
impl SetupSearchQuery {
    pub fn rule(&self) -> RuleProfile {
        self.rule
    }
}
impl SetupSearchQuery {
    pub fn queue(&self) -> &SetupQueueInput {
        &self.queue
    }
}
impl SetupSearchQuery {
    pub fn hold_policy(&self) -> SetupHoldPolicy {
        self.hold_policy
    }
}
impl SetupSearchQuery {
    pub fn piece_budget(&self) -> &PieceBudget {
        &self.piece_budget
    }
}
impl SetupSearchQuery {
    pub fn probability_filter(&self) -> SetupProbabilityFilter {
        self.probability_filter
    }
}
impl SetupSearchQuery {
    pub fn grouping_mode(&self) -> GroupingMode {
        self.grouping_mode
    }
}
impl SetupSearchQuery {
    pub fn limits(&self) -> SetupLimits {
        self.limits
    }
}
impl SetupSearchQuery {
    pub fn residue(&self) -> &SetupResidueInput {
        &self.residue
    }

    pub fn cycle_reset_borrow_policy(&self) -> SetupCycleResetBorrowPolicy {
        self.cycle_reset_borrow_policy
    }

    pub fn candidate_priority(&self) -> SetupCandidatePriority {
        self.candidate_priority
    }

    pub fn length_preference(&self) -> SetupLengthPreference {
        self.length_preference
    }

    pub fn max_setup_pieces(&self) -> u8 {
        self.max_setup_pieces
    }

    pub fn search_mode(&self) -> SetupSearchMode {
        self.search_mode
    }

    pub fn path_detail(&self) -> Option<&SetupPathDetail> {
        self.path_detail.as_ref()
    }
}
impl SetupSearchQuery {
    pub fn with_rule(mut self, rule: RuleProfile) -> Self {
        self.rule = rule;
        self
    }
}
impl SetupSearchQuery {
    pub fn with_queue(mut self, queue: SetupQueueInput) -> Self {
        self.queue = queue;
        self
    }
}
impl SetupSearchQuery {
    pub fn with_hold_policy(mut self, hold_policy: SetupHoldPolicy) -> Self {
        self.hold_policy = hold_policy;
        self
    }
}
impl SetupSearchQuery {
    pub fn with_piece_budget(mut self, piece_budget: PieceBudget) -> Self {
        self.piece_budget = piece_budget;
        self
    }
}
impl SetupSearchQuery {
    pub fn with_probability_filter(mut self, probability_filter: SetupProbabilityFilter) -> Self {
        self.probability_filter = probability_filter;
        self
    }
}
impl SetupSearchQuery {
    pub fn with_grouping_mode(mut self, grouping_mode: GroupingMode) -> Self {
        self.grouping_mode = grouping_mode;
        self
    }
}
impl SetupSearchQuery {
    pub fn with_limits(mut self, limits: SetupLimits) -> Self {
        self.limits = limits;
        self
    }
}
impl SetupSearchQuery {
    pub fn with_remaining_pieces(
        mut self,
        pieces: Vec<clearra_core_domain::piece::piece_kind::PieceKind>,
    ) -> Self {
        self.residue = SetupResidueInput::new(pieces);
        self
    }

    pub fn with_cycle_reset_borrow_policy(mut self, policy: SetupCycleResetBorrowPolicy) -> Self {
        self.cycle_reset_borrow_policy = policy;
        self
    }

    pub fn with_candidate_priority(mut self, priority: SetupCandidatePriority) -> Self {
        self.candidate_priority = priority;
        self
    }

    pub fn with_length_preference(mut self, preference: SetupLengthPreference) -> Self {
        self.length_preference = preference;
        self
    }

    pub fn with_max_setup_pieces(mut self, max_setup_pieces: u8) -> Self {
        self.max_setup_pieces = max_setup_pieces;
        self
    }

    pub fn with_search_mode(mut self, mode: SetupSearchMode) -> Self {
        self.search_mode = mode;
        self
    }

    pub fn with_path_detail(mut self, detail: SetupPathDetail) -> Self {
        self.path_detail = Some(detail);
        self
    }

    pub fn without_path_detail(mut self) -> Self {
        self.path_detail = None;
        self
    }

    pub fn with_next_cycle_remaining_pieces(
        mut self,
        pieces: Vec<clearra_core_domain::piece::piece_kind::PieceKind>,
    ) -> Self {
        self.queue = SetupQueueInput::fixed_sequence(FixedSequence::new(pieces));
        self.search_mode = SetupSearchMode::QueueBased;
        self
    }

    pub fn with_queue_based_pieces(
        self,
        pieces: Vec<clearra_core_domain::piece::piece_kind::PieceKind>,
    ) -> Self {
        self.with_next_cycle_remaining_pieces(pieces)
    }
}

impl Default for SetupSearchQuery {
    fn default() -> Self {
        Self {
            board_size: BoardSize::new(10, 4).expect("fixed setup finder board"),
            target: PcTarget::four_lines(),
            rule: srs_plus(),
            queue: SetupQueueInput::default(),
            hold_policy: SetupHoldPolicy::default(),
            piece_budget: PieceBudget::default(),
            probability_filter: SetupProbabilityFilter::default(),
            grouping_mode: GroupingMode::default(),
            limits: SetupLimits::default(),
            residue: SetupResidueInput::default(),
            cycle_reset_borrow_policy: SetupCycleResetBorrowPolicy::default(),
            candidate_priority: SetupCandidatePriority::default(),
            length_preference: SetupLengthPreference::default(),
            max_setup_pieces: 9,
            search_mode: SetupSearchMode::default(),
            path_detail: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        piece::piece_kind::PieceKind, probability::probability_value::ProbabilityValue,
    };
    use clearra_supply::{hold::hold_slot::HoldSlot, queue::fixed_sequence::FixedSequence};

    use super::*;

    #[test]
    fn query_owns_canonical_setup_contract_parts() {
        let filter = SetupProbabilityFilter::at_least(
            ProbabilityValue::new(0.5).expect("probability threshold"),
        );
        let query = SetupSearchQuery::default()
            .with_queue(SetupQueueInput::fixed_sequence(FixedSequence::new(vec![
                PieceKind::I,
            ])))
            .with_hold_policy(SetupHoldPolicy::enabled_from_slot(HoldSlot::Occupied(
                PieceKind::T,
            )))
            .with_probability_filter(filter)
            .with_grouping_mode(GroupingMode::BuildVariant)
            .with_candidate_priority(SetupCandidatePriority::PcProbabilityFirst)
            .with_limits(SetupLimits::new(1, 2, 3, 4, 5, 6).expect("limits"));

        assert_eq!(query.target(), PcTarget::four_lines());
        assert_eq!(query.rule().id().as_str(), "srs-plus");
        assert!(query.queue().fixed_queue().is_some());
        assert_eq!(query.hold_policy().initial_piece(), Some(PieceKind::T));
        assert_eq!(query.probability_filter(), filter);
        assert_eq!(query.grouping_mode(), GroupingMode::BuildVariant);
        assert_eq!(
            query.candidate_priority(),
            SetupCandidatePriority::PcProbabilityFirst
        );
        assert_eq!(query.limits().max_patterns(), 5);
        assert_eq!(query.limits().post_pc_retained_trace_limit(), 6);
        assert_eq!(query.search_mode(), SetupSearchMode::ShapeOracle);
    }

    #[test]
    fn queue_based_pieces_preserve_order_without_replacing_residue() {
        let query = SetupSearchQuery::default().with_queue_based_pieces(vec![
            PieceKind::T,
            PieceKind::O,
            PieceKind::T,
        ]);

        assert_eq!(query.search_mode(), SetupSearchMode::QueueBased);
        assert_eq!(
            query
                .queue()
                .as_fixed_sequence()
                .expect("fixed queue")
                .pieces(),
            &[PieceKind::T, PieceKind::O, PieceKind::T]
        );
        assert_eq!(query.residue().pieces(), PieceKind::STANDARD_TETROMINOES);
    }

    #[test]
    fn path_detail_can_be_removed_for_graph_cache_identity() {
        let detail = SetupPathDetail::new(1, "hold-empty").expect("detail");
        let base = SetupSearchQuery::default();
        let detailed = base.clone().with_path_detail(detail);

        assert_ne!(detailed, base);
        assert_eq!(detailed.without_path_detail(), base);
    }

    #[test]
    fn setup_piece_limit_defaults_to_nine_and_can_include_the_full_pc() {
        let query = SetupSearchQuery::default();

        assert_eq!(query.max_setup_pieces(), 9);
        assert_eq!(query.with_max_setup_pieces(10).max_setup_pieces(), 10);
    }
}
