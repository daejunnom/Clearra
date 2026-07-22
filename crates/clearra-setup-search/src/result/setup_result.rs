use clearra_core_domain::{
    ids::setup_id::SetupFamilyId, probability::probability_value::ProbabilityValue,
};

use crate::{
    coverage::setup_union_coverage::SetupUnionCoverage,
    evaluate::setup_raw_metrics::SetupRawMetrics,
};

#[derive(Clone, Debug, PartialEq)]
pub struct SetupResult {
    family_id: SetupFamilyId,
    probability: ProbabilityValue,
    union_coverage: SetupUnionCoverage,
    setup_raw_metrics: SetupRawMetrics,
}

impl SetupResult {
    pub fn new(
        family_id: SetupFamilyId,
        probability: ProbabilityValue,
        union_coverage: SetupUnionCoverage,
    ) -> Self {
        Self {
            family_id,
            probability,
            union_coverage,
            setup_raw_metrics: SetupRawMetrics::new(1, 0, 0),
        }
    }
}
impl SetupResult {
    pub fn with_setup_raw_metrics(mut self, setup_raw_metrics: SetupRawMetrics) -> Self {
        self.setup_raw_metrics = setup_raw_metrics;
        self
    }
}
impl SetupResult {
    pub fn family_id(&self) -> SetupFamilyId {
        self.family_id
    }
}
impl SetupResult {
    pub fn probability(&self) -> ProbabilityValue {
        self.probability
    }
}
impl SetupResult {
    pub fn union_coverage(&self) -> &SetupUnionCoverage {
        &self.union_coverage
    }
}
impl SetupResult {
    pub fn setup_raw_metrics(&self) -> &SetupRawMetrics {
        &self.setup_raw_metrics
    }
}
