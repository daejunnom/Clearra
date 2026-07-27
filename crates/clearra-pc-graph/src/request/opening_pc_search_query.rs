use clearra_core_domain::pc::pc_target::PcTarget;
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_profiles::{
    bag::bag_profile::BagProfile, board::board_profile::BoardProfile,
    bundle::standard_profile_bundle::standard_profile_bundle,
    pieces::piece_set_profile::PieceSetProfile,
};
use clearra_rules::{
    kicks::VerifiedKickTableProfile,
    profile::{builtin_rules::srs_plus, rule_profile::RuleProfile},
};
use clearra_supply::QueueObservationPolicy;

use crate::request::{
    pc_execution_policy::PcExecutionPolicy, pc_hold_policy::PcHoldPolicy,
    pc_queue_input::PcQueueInput, PcSolutionProbabilityPolicy, SupplyWindowSize,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpeningPcSearchQuery {
    target: PcTarget,
    board: BoardProfile,
    piece_set: PieceSetProfile,
    bag: BagProfile,
    queue: PcQueueInput,
    hold_policy: PcHoldPolicy,
    supply_window_size: Option<SupplyWindowSize>,
    rule: RuleProfile,
    verified_kick_profile: Option<VerifiedKickTableProfile>,
    objective: ObjectivePolicy,
    solution_probability_policy: PcSolutionProbabilityPolicy,
    queue_observation_policy: QueueObservationPolicy,
    execution_policy: PcExecutionPolicy,
}

impl OpeningPcSearchQuery {
    pub fn new(target: PcTarget) -> Self {
        Self::standard_mvp(target)
    }
}
impl OpeningPcSearchQuery {
    pub fn standard_mvp(target: PcTarget) -> Self {
        let profiles = standard_profile_bundle();
        Self {
            target,
            board: profiles.board(),
            piece_set: profiles.piece_set(),
            bag: profiles.bag(),
            queue: PcQueueInput::default(),
            hold_policy: PcHoldPolicy::default(),
            supply_window_size: None,
            rule: srs_plus(),
            verified_kick_profile: None,
            objective: ObjectivePolicy::all(),
            solution_probability_policy: PcSolutionProbabilityPolicy::Omit,
            queue_observation_policy: QueueObservationPolicy::default(),
            execution_policy: PcExecutionPolicy::mvp_default(),
        }
    }
}
impl OpeningPcSearchQuery {
    pub fn target(&self) -> PcTarget {
        self.target
    }
}
impl OpeningPcSearchQuery {
    pub fn board(&self) -> BoardProfile {
        self.board
    }
}
impl OpeningPcSearchQuery {
    pub fn piece_set(&self) -> PieceSetProfile {
        self.piece_set
    }
}
impl OpeningPcSearchQuery {
    pub fn bag(&self) -> BagProfile {
        self.bag
    }
}
impl OpeningPcSearchQuery {
    pub fn queue(&self) -> &PcQueueInput {
        &self.queue
    }
}
impl OpeningPcSearchQuery {
    pub fn hold_policy(&self) -> PcHoldPolicy {
        self.hold_policy
    }
}
impl OpeningPcSearchQuery {
    pub fn supply_window_size(&self) -> Option<SupplyWindowSize> {
        self.supply_window_size
    }
}
impl OpeningPcSearchQuery {
    pub fn rule(&self) -> RuleProfile {
        self.rule
    }
}
impl OpeningPcSearchQuery {
    pub fn verified_kick_profile(&self) -> Option<&VerifiedKickTableProfile> {
        self.verified_kick_profile.as_ref()
    }
}
impl OpeningPcSearchQuery {
    pub fn objective(&self) -> ObjectivePolicy {
        self.objective
    }
}
impl OpeningPcSearchQuery {
    pub const fn solution_probability_policy(&self) -> PcSolutionProbabilityPolicy {
        self.solution_probability_policy
    }
}
impl OpeningPcSearchQuery {
    pub const fn queue_observation_policy(&self) -> QueueObservationPolicy {
        self.queue_observation_policy
    }
}
impl OpeningPcSearchQuery {
    pub fn execution_policy(&self) -> &PcExecutionPolicy {
        &self.execution_policy
    }
}
impl OpeningPcSearchQuery {
    pub fn with_queue(mut self, queue: PcQueueInput) -> Self {
        self.queue = queue;
        self
    }
}
impl OpeningPcSearchQuery {
    pub fn with_hold_policy(mut self, hold_policy: PcHoldPolicy) -> Self {
        self.hold_policy = hold_policy;
        self
    }
}
impl OpeningPcSearchQuery {
    pub fn with_supply_window_size(mut self, supply_window_size: SupplyWindowSize) -> Self {
        self.supply_window_size = Some(supply_window_size);
        self
    }
}
impl OpeningPcSearchQuery {
    pub fn with_rule(mut self, rule: RuleProfile) -> Self {
        self.rule = rule;
        self.verified_kick_profile = None;
        self
    }
}
impl OpeningPcSearchQuery {
    pub fn with_verified_kick_table_profile(mut self, profile: VerifiedKickTableProfile) -> Self {
        self.rule = RuleProfile::new(profile.profile().source_rule());
        self.verified_kick_profile = Some(profile);
        self
    }
}
impl OpeningPcSearchQuery {
    pub fn with_objective(mut self, objective: ObjectivePolicy) -> Self {
        self.objective = objective;
        self
    }
}
impl OpeningPcSearchQuery {
    pub fn with_solution_probability_policy(mut self, policy: PcSolutionProbabilityPolicy) -> Self {
        self.solution_probability_policy = policy;
        self
    }
}
impl OpeningPcSearchQuery {
    pub fn with_queue_observation_policy(mut self, policy: QueueObservationPolicy) -> Self {
        self.queue_observation_policy = policy;
        self
    }
}
impl OpeningPcSearchQuery {
    pub fn with_execution_policy(mut self, execution_policy: PcExecutionPolicy) -> Self {
        self.execution_policy = execution_policy;
        self
    }
}

#[cfg(test)]
#[path = "opening_pc_search_query_tests.rs"]
mod tests;
