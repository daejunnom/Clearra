use clearra_core_domain::objective::{
    objective_kind::ObjectiveKind, tie_policy::TiePolicy, trace_policy::TracePolicy,
};

use super::execution_constraint_policy::ExecutionConstraintPolicy;
use super::score_objective_policy::{
    ScoreObjectivePolicy, ScoreProfileSelection, SpinProfileSelection,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObjectivePolicy {
    kind: ObjectiveKind,
    tie_policy: TiePolicy,
    trace_policy: TracePolicy,
    score: ScoreObjectivePolicy,
    execution_constraints: ExecutionConstraintPolicy,
}

impl ObjectivePolicy {
    pub fn new(kind: ObjectiveKind, tie_policy: TiePolicy, trace_policy: TracePolicy) -> Self {
        Self {
            kind,
            tie_policy,
            trace_policy,
            score: ScoreObjectivePolicy::DISABLED,
            execution_constraints: ExecutionConstraintPolicy::NONE,
        }
    }
}
impl ObjectivePolicy {
    pub fn all() -> Self {
        Self::new(
            ObjectiveKind::All,
            TiePolicy::StableInputOrder,
            TracePolicy::Keep,
        )
    }
}
impl ObjectivePolicy {
    pub fn unique() -> Self {
        Self::new(
            ObjectiveKind::Unique,
            TiePolicy::StableInputOrder,
            TracePolicy::Keep,
        )
    }
}
impl ObjectivePolicy {
    pub fn minimum_cover() -> Self {
        Self::new(
            ObjectiveKind::MinimumCover,
            TiePolicy::StableInputOrder,
            TracePolicy::Keep,
        )
    }
}
impl ObjectivePolicy {
    pub const fn with_score_policy(mut self, score: ScoreObjectivePolicy) -> Self {
        self.score = score;
        self
    }
}
impl ObjectivePolicy {
    pub const fn with_score_summary(self) -> Self {
        self.with_score_policy(ScoreObjectivePolicy::summary())
    }
}
impl ObjectivePolicy {
    pub const fn with_initial_b2b(mut self, initial_b2b: u32) -> Self {
        let score = if self.score.requested() {
            self.score
        } else {
            ScoreObjectivePolicy::summary()
        };
        self.score = score.with_initial_b2b(initial_b2b);
        self
    }

    pub const fn with_score_profile(mut self, profile: ScoreProfileSelection) -> Self {
        let score = if self.score.requested() {
            self.score
        } else {
            ScoreObjectivePolicy::summary()
        };
        self.score = score.with_profile(profile);
        self
    }

    pub const fn with_spin_profile(mut self, profile: SpinProfileSelection) -> Self {
        let score = if self.score.requested() {
            self.score
        } else {
            ScoreObjectivePolicy::summary()
        };
        self.score = score.with_spin_profile(profile);
        self
    }

    pub const fn with_back_to_back_preservation(
        mut self,
        spin_profile: SpinProfileSelection,
    ) -> Self {
        self.execution_constraints = ExecutionConstraintPolicy::preserve_back_to_back(spin_profile);
        self
    }
}
impl ObjectivePolicy {
    pub fn kind(self) -> ObjectiveKind {
        self.kind
    }
}
impl ObjectivePolicy {
    pub fn tie_policy(self) -> TiePolicy {
        self.tie_policy
    }
}
impl ObjectivePolicy {
    pub fn trace_policy(self) -> TracePolicy {
        self.trace_policy
    }
}
impl ObjectivePolicy {
    pub const fn score(self) -> ScoreObjectivePolicy {
        self.score
    }

    pub const fn execution_constraints(self) -> ExecutionConstraintPolicy {
        self.execution_constraints
    }
}

impl Default for ObjectivePolicy {
    fn default() -> Self {
        Self::all()
    }
}
