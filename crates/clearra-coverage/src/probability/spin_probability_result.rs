use clearra_core_domain::{ids::SpinTargetId, probability::probability_value::ProbabilityValue};

use crate::universe::{
    pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
};

#[derive(Clone, Debug, PartialEq)]
pub struct SpinProbabilityResult {
    spin_target_id: SpinTargetId,
    covered_pattern_count: usize,
    pattern_count: usize,
    pattern_universe_id: PatternUniverseId,
    pattern_weight_model_id: PatternWeightModelId,
    probability: ProbabilityValue,
    probability_complete: bool,
    materialized_probability_mass: ProbabilityValue,
    renormalized: bool,
    truncation_reason: Option<String>,
}

impl SpinProbabilityResult {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        spin_target_id: SpinTargetId,
        covered_pattern_count: usize,
        pattern_count: usize,
        pattern_universe_id: PatternUniverseId,
        pattern_weight_model_id: PatternWeightModelId,
        probability: ProbabilityValue,
        probability_complete: bool,
        materialized_probability_mass: ProbabilityValue,
        renormalized: bool,
        truncation_reason: Option<String>,
    ) -> Self {
        Self {
            spin_target_id,
            covered_pattern_count,
            pattern_count,
            pattern_universe_id,
            pattern_weight_model_id,
            probability,
            probability_complete,
            materialized_probability_mass,
            renormalized,
            truncation_reason,
        }
    }
}
impl SpinProbabilityResult {
    pub fn spin_target_id(&self) -> &SpinTargetId {
        &self.spin_target_id
    }
}
impl SpinProbabilityResult {
    pub fn covered_pattern_count(&self) -> usize {
        self.covered_pattern_count
    }
}
impl SpinProbabilityResult {
    pub fn pattern_count(&self) -> usize {
        self.pattern_count
    }
}
impl SpinProbabilityResult {
    pub fn pattern_universe_id(&self) -> PatternUniverseId {
        self.pattern_universe_id
    }
}
impl SpinProbabilityResult {
    pub fn pattern_weight_model_id(&self) -> PatternWeightModelId {
        self.pattern_weight_model_id
    }
}
impl SpinProbabilityResult {
    pub fn probability(&self) -> ProbabilityValue {
        self.probability
    }
}
impl SpinProbabilityResult {
    pub fn probability_complete(&self) -> bool {
        self.probability_complete
    }
}
impl SpinProbabilityResult {
    pub fn materialized_probability_mass(&self) -> ProbabilityValue {
        self.materialized_probability_mass
    }
}
impl SpinProbabilityResult {
    pub fn renormalized(&self) -> bool {
        self.renormalized
    }
}
impl SpinProbabilityResult {
    pub fn truncation_reason(&self) -> Option<&str> {
        self.truncation_reason.as_deref()
    }
}
