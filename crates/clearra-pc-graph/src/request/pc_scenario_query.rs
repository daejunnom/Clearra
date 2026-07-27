use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_profiles::{
    bag::bag_profile::BagProfile, bundle::standard_profile_bundle::standard_profile_bundle,
    pieces::piece_set_profile::PieceSetProfile, search::search_defaults::SearchDefaults,
};
use clearra_rules::{
    kicks::VerifiedKickTableProfile,
    profile::{builtin_rules::srs_plus, rule_profile::RuleProfile},
};
use clearra_supply::{hold::hold_slot::HoldSlot, QueueObservationPolicy};

use crate::request::{
    extended_pc_scenario_board::ExtendedPcScenarioBoard, pc_execution_policy::PcExecutionPolicy,
    pc_queue_input::PcQueueInput, PcSolutionProbabilityPolicy,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcScenarioBoard {
    width: u16,
    visible_height: u16,
    occupied_mask: u64,
}

impl PcScenarioBoard {
    pub fn new(width: u16, visible_height: u16, occupied_mask: u64) -> Self {
        Self {
            width,
            visible_height,
            occupied_mask,
        }
    }
}
impl PcScenarioBoard {
    pub fn standard_10(visible_height: u16, occupied_mask: u64) -> Self {
        Self::new(10, visible_height, occupied_mask)
    }
}
impl PcScenarioBoard {
    pub fn width(&self) -> u16 {
        self.width
    }
}
impl PcScenarioBoard {
    pub fn visible_height(&self) -> u16 {
        self.visible_height
    }
}
impl PcScenarioBoard {
    pub fn occupied_mask(&self) -> u64 {
        self.occupied_mask
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PieceWindow {
    max_pieces: usize,
}

impl PieceWindow {
    pub fn new(max_pieces: usize) -> Self {
        Self { max_pieces }
    }
}
impl PieceWindow {
    pub fn max_pieces(self) -> usize {
        self.max_pieces
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PcCompletionGoal {
    #[default]
    ClearToEmpty,
}

impl PcCompletionGoal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClearToEmpty => "clear-to-empty",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PcCountPolicy {
    FirstSolution,
    #[default]
    CountAll,
    CountUnique,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SupplyWindowSize {
    source_pieces: usize,
}

impl SupplyWindowSize {
    pub const fn new(source_pieces: usize) -> Self {
        Self { source_pieces }
    }

    pub const fn source_pieces(self) -> usize {
        self.source_pieces
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcScenarioQuery<B = PcScenarioBoard> {
    initial_board: B,
    remaining_queue: PcQueueInput,
    hold_state: HoldSlot,
    piece_set: PieceSetProfile,
    bag: BagProfile,
    rule: RuleProfile,
    verified_kick_profile: Option<VerifiedKickTableProfile>,
    piece_window: PieceWindow,
    exact_pieces: Option<usize>,
    min_remaining_queue: usize,
    allow_hold: bool,
    supply_window_size: Option<SupplyWindowSize>,
    requires_180: bool,
    completion_goal: PcCompletionGoal,
    count_policy: PcCountPolicy,
    retained_trace_limit: usize,
    objective: ObjectivePolicy,
    solution_probability_policy: PcSolutionProbabilityPolicy,
    queue_observation_policy: QueueObservationPolicy,
    execution_policy: PcExecutionPolicy,
}

pub type ExtendedPcScenarioQuery = PcScenarioQuery<ExtendedPcScenarioBoard>;

impl PcScenarioQuery<PcScenarioBoard> {
    pub fn new(
        initial_board: PcScenarioBoard,
        remaining_queue: PcQueueInput,
        piece_window: PieceWindow,
    ) -> Self {
        Self::new_with_board(initial_board, remaining_queue, piece_window)
    }
}

impl PcScenarioQuery<ExtendedPcScenarioBoard> {
    pub fn new_extended(
        initial_board: ExtendedPcScenarioBoard,
        remaining_queue: PcQueueInput,
        piece_window: PieceWindow,
    ) -> Self {
        Self::new_with_board(initial_board, remaining_queue, piece_window)
    }
}

impl<B> PcScenarioQuery<B> {
    fn new_with_board(
        initial_board: B,
        remaining_queue: PcQueueInput,
        piece_window: PieceWindow,
    ) -> Self {
        let profiles = standard_profile_bundle();
        Self {
            initial_board,
            remaining_queue,
            hold_state: HoldSlot::Empty,
            piece_set: profiles.piece_set(),
            bag: profiles.bag(),
            rule: srs_plus(),
            verified_kick_profile: None,
            piece_window,
            exact_pieces: None,
            min_remaining_queue: 0,
            allow_hold: true,
            supply_window_size: None,
            requires_180: false,
            completion_goal: PcCompletionGoal::ClearToEmpty,
            count_policy: PcCountPolicy::CountAll,
            retained_trace_limit: SearchDefaults::MVP1.scenario_retained_trace_limit(),
            objective: ObjectivePolicy::all(),
            solution_probability_policy: PcSolutionProbabilityPolicy::Omit,
            queue_observation_policy: QueueObservationPolicy::default(),
            execution_policy: PcExecutionPolicy::mvp_default(),
        }
    }

    pub fn initial_board(&self) -> &B {
        &self.initial_board
    }

    pub fn remaining_queue(&self) -> &PcQueueInput {
        &self.remaining_queue
    }

    pub fn hold_state(&self) -> HoldSlot {
        self.hold_state
    }

    pub fn piece_set(&self) -> PieceSetProfile {
        self.piece_set
    }

    pub fn bag(&self) -> BagProfile {
        self.bag
    }

    pub fn rule(&self) -> RuleProfile {
        self.rule
    }

    pub fn verified_kick_profile(&self) -> Option<&VerifiedKickTableProfile> {
        self.verified_kick_profile.as_ref()
    }

    pub fn piece_window(&self) -> PieceWindow {
        self.piece_window
    }

    pub fn exact_pieces(&self) -> Option<usize> {
        self.exact_pieces
    }

    pub fn min_remaining_queue(&self) -> usize {
        self.min_remaining_queue
    }

    pub fn allow_hold(&self) -> bool {
        self.allow_hold
    }

    pub fn supply_window_size(&self) -> Option<SupplyWindowSize> {
        self.supply_window_size
    }

    pub fn requires_180(&self) -> bool {
        self.requires_180
    }

    pub fn completion_goal(&self) -> PcCompletionGoal {
        self.completion_goal
    }

    pub fn count_policy(&self) -> PcCountPolicy {
        if self.objective.score().requested() {
            PcCountPolicy::CountAll
        } else {
            self.count_policy
        }
    }

    pub fn retained_trace_limit(&self) -> usize {
        self.retained_trace_limit
    }

    pub fn objective(&self) -> ObjectivePolicy {
        self.objective
    }

    pub const fn solution_probability_policy(&self) -> PcSolutionProbabilityPolicy {
        self.solution_probability_policy
    }

    pub const fn queue_observation_policy(&self) -> QueueObservationPolicy {
        self.queue_observation_policy
    }

    pub fn execution_policy(&self) -> &PcExecutionPolicy {
        &self.execution_policy
    }

    pub fn with_hold_piece(mut self, piece: Option<PieceKind>) -> Self {
        self.hold_state = match piece {
            Some(piece) => HoldSlot::Occupied(piece),
            None => HoldSlot::Empty,
        };
        self
    }

    pub fn with_exact_pieces(mut self, exact_pieces: Option<usize>) -> Self {
        self.exact_pieces = exact_pieces;
        self
    }

    pub fn with_min_remaining_queue(mut self, min_remaining_queue: usize) -> Self {
        self.min_remaining_queue = min_remaining_queue;
        self
    }

    pub fn with_allow_hold(mut self, allow_hold: bool) -> Self {
        self.allow_hold = allow_hold;
        self
    }

    pub fn with_supply_window_size(mut self, supply_window_size: SupplyWindowSize) -> Self {
        self.supply_window_size = Some(supply_window_size);
        self
    }

    pub fn with_requires_180(mut self, requires_180: bool) -> Self {
        self.requires_180 = requires_180;
        self
    }

    pub fn with_rule(mut self, rule: RuleProfile) -> Self {
        self.rule = rule;
        self.verified_kick_profile = None;
        self
    }

    pub fn with_verified_kick_table_profile(mut self, profile: VerifiedKickTableProfile) -> Self {
        self.rule = RuleProfile::new(profile.profile().source_rule());
        self.verified_kick_profile = Some(profile);
        self
    }

    pub fn with_count_policy(mut self, count_policy: PcCountPolicy) -> Self {
        self.count_policy = count_policy;
        self.objective = match count_policy {
            PcCountPolicy::FirstSolution | PcCountPolicy::CountAll => ObjectivePolicy::all(),
            PcCountPolicy::CountUnique => ObjectivePolicy::unique(),
        };
        self
    }

    pub fn with_objective(mut self, objective: ObjectivePolicy) -> Self {
        self.objective = objective;
        if objective.score().requested() {
            self.count_policy = PcCountPolicy::CountAll;
        }
        self
    }

    pub fn with_solution_probability_policy(mut self, policy: PcSolutionProbabilityPolicy) -> Self {
        self.solution_probability_policy = policy;
        self
    }

    pub fn with_queue_observation_policy(mut self, policy: QueueObservationPolicy) -> Self {
        self.queue_observation_policy = policy;
        self
    }

    pub fn with_retained_trace_limit(mut self, retained_trace_limit: usize) -> Self {
        self.retained_trace_limit = retained_trace_limit;
        self
    }

    pub fn with_execution_policy(mut self, execution_policy: PcExecutionPolicy) -> Self {
        self.execution_policy = execution_policy;
        self
    }
}

#[cfg(test)]
#[path = "pc_scenario_query_tests.rs"]
mod tests;
