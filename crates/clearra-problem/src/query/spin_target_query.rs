use clearra_pc_graph::request::{OpeningPcSearchQuery, PcScenarioQuery};

use crate::goal::SpinTargetRequest;

use super::setup_query::SetupSearchQuery;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpinTargetQuerySource {
    PercentGoalSpin,
    SetupGoalSpin,
    PcThenSpin,
}

impl SpinTargetQuerySource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PercentGoalSpin => "percent-goal-spin",
            Self::SetupGoalSpin => "setup-goal-spin",
            Self::PcThenSpin => "pc-then-spin",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SpinTargetBaseQuery {
    Percent(PcScenarioQuery),
    Setup(SetupSearchQuery),
    PcThenSpin(OpeningPcSearchQuery),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SpinTargetTraceRequirement {
    #[default]
    BuildVariantReplayEvidence,
    KickEvidenceRequired,
    FullReplayTrace,
}

impl SpinTargetTraceRequirement {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::BuildVariantReplayEvidence => "build-variant-replay-evidence",
            Self::KickEvidenceRequired => "kick-evidence-required",
            Self::FullReplayTrace => "full-replay-trace",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SpinTargetQuery {
    source: SpinTargetQuerySource,
    base_query: SpinTargetBaseQuery,
    spin_target: SpinTargetRequest,
    trace_requirement: SpinTargetTraceRequirement,
}

impl SpinTargetQuery {
    pub fn percent_goal_spin(base_query: PcScenarioQuery, spin_target: SpinTargetRequest) -> Self {
        Self {
            source: SpinTargetQuerySource::PercentGoalSpin,
            base_query: SpinTargetBaseQuery::Percent(base_query),
            spin_target,
            trace_requirement: SpinTargetTraceRequirement::BuildVariantReplayEvidence,
        }
    }
}
impl SpinTargetQuery {
    pub fn setup_goal_spin(base_query: SetupSearchQuery, spin_target: SpinTargetRequest) -> Self {
        Self {
            source: SpinTargetQuerySource::SetupGoalSpin,
            base_query: SpinTargetBaseQuery::Setup(base_query),
            spin_target,
            trace_requirement: SpinTargetTraceRequirement::BuildVariantReplayEvidence,
        }
    }
}
impl SpinTargetQuery {
    pub fn pc_then_spin(base_query: OpeningPcSearchQuery, spin_target: SpinTargetRequest) -> Self {
        Self {
            source: SpinTargetQuerySource::PcThenSpin,
            base_query: SpinTargetBaseQuery::PcThenSpin(base_query),
            spin_target,
            trace_requirement: SpinTargetTraceRequirement::BuildVariantReplayEvidence,
        }
    }
}
impl SpinTargetQuery {
    pub fn with_trace_requirement(mut self, trace_requirement: SpinTargetTraceRequirement) -> Self {
        self.trace_requirement = trace_requirement;
        self
    }
}
impl SpinTargetQuery {
    pub fn source(&self) -> SpinTargetQuerySource {
        self.source
    }
}
impl SpinTargetQuery {
    pub fn base_query(&self) -> &SpinTargetBaseQuery {
        &self.base_query
    }
}
impl SpinTargetQuery {
    pub fn spin_target(&self) -> &SpinTargetRequest {
        &self.spin_target
    }
}
impl SpinTargetQuery {
    pub fn score_profile_id(&self) -> Option<&str> {
        self.spin_target.required_score_profile()
    }
}
impl SpinTargetQuery {
    pub fn target_probability_threshold(
        &self,
    ) -> Option<clearra_core_domain::probability::probability_value::ProbabilityValue> {
        self.spin_target.target_probability_threshold()
    }
}
impl SpinTargetQuery {
    pub fn trace_requirement(&self) -> SpinTargetTraceRequirement {
        self.trace_requirement
    }
}
