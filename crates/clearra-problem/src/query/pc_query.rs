use clearra_core_domain::pc::pc_target::PcTarget;
use clearra_objectives::policy::objective_policy::ObjectivePolicy;
use clearra_pc_graph::request::{
    OpeningPcSearchQuery, PcCountPolicy, PcExecutionPolicy, PcHoldPolicy, PcQueueInput,
    PcSolutionProbabilityPolicy, SupplyWindowSize,
};
use clearra_profiles::{
    bag::bag_profile::BagProfile, board::board_profile::BoardProfile,
    pieces::piece_set_profile::PieceSetProfile,
};
use clearra_rules::{kicks::VerifiedKickTableProfile, profile::rule_profile::RuleProfile};
use clearra_supply::QueueObservationPolicy;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcQuery {
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
    count_policy: PcCountPolicy,
    solution_probability_policy: PcSolutionProbabilityPolicy,
    queue_observation_policy: QueueObservationPolicy,
    execution_policy: PcExecutionPolicy,
}

impl PcQuery {
    pub fn from_opening_query(query: &OpeningPcSearchQuery) -> Self {
        Self {
            target: query.target(),
            board: query.board(),
            piece_set: query.piece_set(),
            bag: query.bag(),
            queue: query.queue().clone(),
            hold_policy: query.hold_policy(),
            supply_window_size: query.supply_window_size(),
            rule: query.rule(),
            verified_kick_profile: query.verified_kick_profile().cloned(),
            objective: query.objective(),
            count_policy: query.count_policy(),
            solution_probability_policy: query.solution_probability_policy(),
            queue_observation_policy: query.queue_observation_policy(),
            execution_policy: query.execution_policy().clone(),
        }
    }
}
impl PcQuery {
    pub fn target(&self) -> PcTarget {
        self.target
    }
}
impl PcQuery {
    pub fn board(&self) -> BoardProfile {
        self.board
    }
}
impl PcQuery {
    pub fn piece_set(&self) -> PieceSetProfile {
        self.piece_set
    }
}
impl PcQuery {
    pub fn bag(&self) -> BagProfile {
        self.bag
    }
}
impl PcQuery {
    pub fn queue(&self) -> &PcQueueInput {
        &self.queue
    }
}
impl PcQuery {
    pub fn hold_policy(&self) -> PcHoldPolicy {
        self.hold_policy
    }
}
impl PcQuery {
    pub fn supply_window_size(&self) -> Option<SupplyWindowSize> {
        self.supply_window_size
    }
}
impl PcQuery {
    pub fn rule(&self) -> RuleProfile {
        self.rule
    }
}
impl PcQuery {
    pub fn verified_kick_profile(&self) -> Option<&VerifiedKickTableProfile> {
        self.verified_kick_profile.as_ref()
    }
}
impl PcQuery {
    pub fn objective(&self) -> ObjectivePolicy {
        self.objective
    }
}
impl PcQuery {
    pub const fn solution_probability_policy(&self) -> PcSolutionProbabilityPolicy {
        self.solution_probability_policy
    }
}
impl PcQuery {
    pub const fn queue_observation_policy(&self) -> QueueObservationPolicy {
        self.queue_observation_policy
    }
}
impl PcQuery {
    pub fn execution_policy(&self) -> &PcExecutionPolicy {
        &self.execution_policy
    }
}
impl PcQuery {
    pub fn opening_label(&self) -> String {
        format!("{}L", self.target.lines())
    }
}
impl PcQuery {
    pub fn opening_labels(&self) -> Vec<String> {
        [2_u8, 4, 6]
            .into_iter()
            .filter(|lines| *lines <= self.target.lines())
            .map(|lines| format!("{lines}L"))
            .collect()
    }
}
impl PcQuery {
    pub fn exact_piece_count(&self) -> usize {
        (usize::from(self.target.lines()) * 10) / 4
    }
}
impl PcQuery {
    pub fn count_policy(&self) -> PcCountPolicy {
        if self.objective.score().requested() {
            return PcCountPolicy::CountAll;
        }
        self.count_policy
    }
}
impl PcQuery {
    pub fn to_opening_query(&self) -> OpeningPcSearchQuery {
        let mut query = OpeningPcSearchQuery::new(self.target)
            .with_queue(self.queue.clone())
            .with_hold_policy(self.hold_policy)
            .with_rule(self.rule)
            .with_objective(self.objective)
            .with_count_policy(self.count_policy)
            .with_solution_probability_policy(self.solution_probability_policy)
            .with_queue_observation_policy(self.queue_observation_policy)
            .with_execution_policy(self.execution_policy.clone());
        if let Some(supply_window_size) = self.supply_window_size {
            query = query.with_supply_window_size(supply_window_size);
        }
        if let Some(profile) = &self.verified_kick_profile {
            query = query.with_verified_kick_table_profile(profile.clone());
        }
        query
    }
}
