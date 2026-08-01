use clearra_core_domain::{
    piece::piece_kind::PieceKind, solution::StandardBoard64ColoredTilingIdentity,
};
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    PcCountPolicy, PcExecutionPolicy, PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow,
    SupplyWindowSize,
};
use clearra_rules::profile::rule_profile::RuleProfile;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebPcScenarioInput {
    board_mask: u64,
    visible_height: u16,
    piece_window: usize,
    hold_piece: Option<PieceKind>,
    allow_hold: bool,
    supply_window_size: Option<SupplyWindowSize>,
    count_policy: PcCountPolicy,
    retained_trace_limit: usize,
    allowed_colored_solution_identities: Option<Vec<StandardBoard64ColoredTilingIdentity>>,
}

impl WebPcScenarioInput {
    pub fn new(board_mask: u64, visible_height: u16, piece_window: usize) -> Self {
        Self {
            board_mask,
            visible_height,
            piece_window,
            hold_piece: None,
            allow_hold: true,
            supply_window_size: None,
            count_policy: PcCountPolicy::CountUnique,
            retained_trace_limit: 1,
            allowed_colored_solution_identities: None,
        }
    }

    pub fn with_hold_piece(mut self, hold_piece: Option<PieceKind>) -> Self {
        self.hold_piece = hold_piece;
        self
    }

    pub fn with_allow_hold(mut self, allow_hold: bool) -> Self {
        self.allow_hold = allow_hold;
        self
    }

    pub fn with_source_piece_count(mut self, source_piece_count: usize) -> Self {
        self.supply_window_size = Some(SupplyWindowSize::new(source_piece_count));
        self
    }

    pub fn with_count_policy(mut self, count_policy: PcCountPolicy) -> Self {
        self.count_policy = count_policy;
        self
    }

    pub fn with_retained_trace_limit(mut self, retained_trace_limit: usize) -> Self {
        self.retained_trace_limit = retained_trace_limit;
        self
    }

    pub fn with_allowed_colored_solution_identities(
        mut self,
        identities: impl IntoIterator<Item = StandardBoard64ColoredTilingIdentity>,
    ) -> Self {
        self.allowed_colored_solution_identities = Some(identities.into_iter().collect());
        self
    }

    pub fn to_query(
        &self,
        queue: PcQueueInput,
        execution_policy: PcExecutionPolicy,
        finite_standard_bag_len: Option<usize>,
        rule: RuleProfile,
        objective: ObjectivePolicy,
    ) -> PcScenarioQuery {
        let mut query = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(self.visible_height, self.board_mask),
            queue,
            PieceWindow::new(self.piece_window),
        )
        .with_rule(rule)
        .with_exact_pieces(Some(self.piece_window))
        .with_min_remaining_queue(0)
        .with_hold_piece(self.hold_piece)
        .with_allow_hold(self.allow_hold)
        .with_count_policy(self.count_policy)
        .with_objective(objective)
        .with_retained_trace_limit(self.retained_trace_limit)
        .with_execution_policy(execution_policy);
        // An occupied initial hold contributes one independently supplied
        // piece. It is not removed from the queue or bag expression.
        let initial_hold_prefix = usize::from(self.allow_hold && self.hold_piece.is_some());
        let automatic_source_pieces = self
            .piece_window
            .saturating_add(usize::from(self.allow_hold))
            .saturating_sub(initial_hold_prefix);
        let finite_standard_bag_window = finite_standard_bag_len
            .map(|length| SupplyWindowSize::new(length.min(automatic_source_pieces)));
        if let Some(supply_window_size) = self.supply_window_size.or(finite_standard_bag_window) {
            query = query.with_supply_window_size(supply_window_size);
        }
        if let Some(identities) = self.allowed_colored_solution_identities.clone() {
            query = query.with_allowed_colored_solution_identities(identities);
        }
        query
    }

    pub const fn board_mask(&self) -> u64 {
        self.board_mask
    }

    pub const fn visible_height(&self) -> u16 {
        self.visible_height
    }

    pub const fn piece_window(&self) -> usize {
        self.piece_window
    }

    pub const fn hold_piece(&self) -> Option<PieceKind> {
        self.hold_piece
    }
}
