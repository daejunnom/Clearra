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

use clearra_core_domain::{
    board::board_size::BoardSize, pc::pc_target::PcTarget, piece::piece_kind::PieceKind,
};
use clearra_rules::profile::{builtin_rules::srs_plus, rule_profile::RuleProfile};
use clearra_supply::{queue::fixed_sequence::FixedSequence, QueueObservationPolicy};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupPathDetail {
    board_mask: u64,
    deleted_rows: u16,
    placement_rows: u128,
    condition_id: String,
}

impl SetupPathDetail {
    pub fn new(
        board_mask: u64,
        deleted_rows: u16,
        placement_rows: u128,
        condition_id: impl Into<String>,
    ) -> Option<Self> {
        let condition_id = condition_id.into();
        (board_mask >> 40 == 0
            && placement_rows != 0
            && placement_rows >> 120 == 0
            && !condition_id.is_empty())
        .then_some(Self {
            board_mask,
            deleted_rows,
            placement_rows,
            condition_id,
        })
    }

    pub fn from_setup_id(setup_id: &str, condition_id: impl Into<String>) -> Option<Self> {
        let mut components = setup_id.split('-');
        if components.next()? != "setup" {
            return None;
        }
        let board = components.next()?;
        let deleted_rows = components.next()?;
        let placement_rows = components.next()?;
        if components.next().is_some()
            || board.len() != 10
            || deleted_rows.len() != 4
            || placement_rows.len() != 30
        {
            return None;
        }
        Self::new(
            u64::from_str_radix(board, 16).ok()?,
            u16::from_str_radix(deleted_rows, 16).ok()?,
            u128::from_str_radix(placement_rows, 16).ok()?,
            condition_id,
        )
    }

    pub fn setup_id_for(
        board_mask: u64,
        deleted_rows: u16,
        placement_rows: u128,
    ) -> Option<String> {
        Self::new(board_mask, deleted_rows, placement_rows, "identity")
            .map(|detail| detail.setup_id())
    }

    pub fn setup_id(&self) -> String {
        format!(
            "setup-{:010x}-{:04x}-{:030x}",
            self.board_mask, self.deleted_rows, self.placement_rows
        )
    }

    pub const fn board_mask(&self) -> u64 {
        self.board_mask
    }

    pub const fn deleted_rows(&self) -> u16 {
        self.deleted_rows
    }

    pub const fn placement_rows(&self) -> u128 {
        self.placement_rows
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
    queue_observation_policy: QueueObservationPolicy,
    next_cycle_remaining_pieces: Option<Vec<PieceKind>>,
    path_detail: Option<SetupPathDetail>,
    tablebase_requested: bool,
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
            queue_observation_policy: QueueObservationPolicy::default(),
            next_cycle_remaining_pieces: None,
            path_detail: None,
            tablebase_requested: false,
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

    pub fn queue_observation_policy(&self) -> QueueObservationPolicy {
        self.queue_observation_policy
    }

    pub fn next_cycle_remaining_pieces(&self) -> Option<&[PieceKind]> {
        self.next_cycle_remaining_pieces.as_deref()
    }

    pub fn path_detail(&self) -> Option<&SetupPathDetail> {
        self.path_detail.as_ref()
    }

    pub const fn tablebase_requested(&self) -> bool {
        self.tablebase_requested
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

    pub fn with_queue_observation_policy(mut self, policy: QueueObservationPolicy) -> Self {
        self.queue_observation_policy = policy;
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

    pub fn with_tablebase_requested(mut self, requested: bool) -> Self {
        self.tablebase_requested = requested;
        self
    }

    pub fn with_next_cycle_remaining_pieces(mut self, pieces: Vec<PieceKind>) -> Self {
        self.next_cycle_remaining_pieces = Some(pieces);
        self
    }

    pub fn with_queue_based_pieces(mut self, pieces: Vec<PieceKind>) -> Self {
        self.queue = SetupQueueInput::fixed_sequence(FixedSequence::new(pieces));
        self.search_mode = SetupSearchMode::QueueBased;
        self
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
            queue_observation_policy: QueueObservationPolicy::default(),
            next_cycle_remaining_pieces: None,
            path_detail: None,
            tablebase_requested: false,
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
        assert!(query.next_cycle_remaining_pieces().is_none());
    }

    #[test]
    fn next_cycle_inventory_does_not_change_search_mode_or_observed_queue() {
        let query = SetupSearchQuery::default()
            .with_next_cycle_remaining_pieces(vec![PieceKind::O, PieceKind::S]);

        assert_eq!(query.search_mode(), SetupSearchMode::ShapeOracle);
        assert!(query.queue().as_fixed_sequence().is_none());
        assert_eq!(
            query.next_cycle_remaining_pieces(),
            Some(&[PieceKind::O, PieceKind::S][..])
        );
    }

    #[test]
    fn observed_queue_and_next_cycle_inventory_can_coexist() {
        let query = SetupSearchQuery::default()
            .with_queue_based_pieces(vec![PieceKind::O, PieceKind::S])
            .with_next_cycle_remaining_pieces(vec![PieceKind::T]);

        assert_eq!(query.search_mode(), SetupSearchMode::QueueBased);
        assert_eq!(
            query
                .queue()
                .as_fixed_sequence()
                .expect("observed queue")
                .pieces(),
            &[PieceKind::O, PieceKind::S]
        );
        assert_eq!(
            query.next_cycle_remaining_pieces(),
            Some(&[PieceKind::T][..])
        );
    }

    #[test]
    fn path_detail_can_be_removed_for_graph_cache_identity() {
        let detail = SetupPathDetail::new(1, 0, 1, "hold-empty").expect("detail");
        let base = SetupSearchQuery::default();
        let detailed = base.clone().with_path_detail(detail);

        assert_ne!(detailed, base);
        assert_eq!(detailed.without_path_detail(), base);
    }

    #[test]
    fn setup_path_detail_round_trips_exact_partial_state_identity() {
        let detail = SetupPathDetail::new(
            0x0008_0719_e6,
            0x0012,
            0x0000_0000_0000_0042_1003_2007,
            "hold-empty",
        )
        .expect("detail");
        let setup_id = detail.setup_id();
        let parsed =
            SetupPathDetail::from_setup_id(&setup_id, "hold-empty").expect("parsed detail");

        assert_eq!(parsed, detail);
        assert_eq!(
            setup_id,
            "setup-00080719e6-0012-000000000000000000004210032007"
        );
    }

    #[test]
    fn setup_path_detail_rejects_identity_bits_outside_wire_format() {
        assert!(SetupPathDetail::new(1, 0, 1_u128 << 120, "hold-empty").is_none());
    }

    #[test]
    fn setup_piece_limit_defaults_to_nine_and_can_include_the_full_pc() {
        let query = SetupSearchQuery::default();

        assert_eq!(query.max_setup_pieces(), 9);
        assert_eq!(query.with_max_setup_pieces(10).max_setup_pieces(), 10);
    }
}
