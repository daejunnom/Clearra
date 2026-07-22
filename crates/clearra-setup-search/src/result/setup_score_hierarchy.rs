use clearra_core_domain::{
    ids::setup_id::{SetupFamilyId, TilingVariantId},
    probability::probability_value::ProbabilityValue,
};

use crate::evaluate::ScoreEvaluationBasis;

use super::{setup_score_aggregation::SetupScoreTotals, SetupBuildScore};

#[derive(Clone, Debug, PartialEq)]
pub struct SetupTilingScore {
    tiling_variant_id: TilingVariantId,
    coverage_probability: ProbabilityValue,
    post_pc_probability: ProbabilityValue,
    expected_score: f64,
    expected_attack: f64,
    total_solution_count: usize,
    score_evaluation_trace_count: usize,
    score_evaluation_complete: bool,
    score_evaluation_basis: ScoreEvaluationBasis,
    continuation_available: bool,
    continuation_available_complete: bool,
    build_variants: Vec<SetupBuildScore>,
}

impl SetupTilingScore {
    pub(super) fn new(
        tiling_variant_id: TilingVariantId,
        totals: SetupScoreTotals,
        build_variants: Vec<SetupBuildScore>,
    ) -> Self {
        Self {
            tiling_variant_id,
            coverage_probability: totals.coverage_probability,
            post_pc_probability: totals.post_pc_probability,
            expected_score: totals.expected_score,
            expected_attack: totals.expected_attack,
            total_solution_count: totals.total_solution_count,
            score_evaluation_trace_count: totals.score_evaluation_trace_count,
            score_evaluation_complete: totals.score_evaluation_complete,
            score_evaluation_basis: totals.score_evaluation_basis,
            continuation_available: totals.continuation_available,
            continuation_available_complete: totals.continuation_available_complete,
            build_variants,
        }
    }
}
impl SetupTilingScore {
    pub fn tiling_variant_id(&self) -> TilingVariantId {
        self.tiling_variant_id
    }
}
impl SetupTilingScore {
    pub fn coverage_probability(&self) -> ProbabilityValue {
        self.coverage_probability
    }
}
impl SetupTilingScore {
    pub fn post_pc_probability(&self) -> ProbabilityValue {
        self.post_pc_probability
    }
}
impl SetupTilingScore {
    pub fn expected_score(&self) -> f64 {
        self.expected_score
    }
}
impl SetupTilingScore {
    pub fn expected_attack(&self) -> f64 {
        self.expected_attack
    }
}
impl SetupTilingScore {
    pub fn total_solution_count(&self) -> usize {
        self.total_solution_count
    }
}
impl SetupTilingScore {
    pub fn score_evaluation_trace_count(&self) -> usize {
        self.score_evaluation_trace_count
    }
}
impl SetupTilingScore {
    pub fn score_evaluation_complete(&self) -> bool {
        self.score_evaluation_complete
    }
}
impl SetupTilingScore {
    pub fn score_evaluation_basis(&self) -> ScoreEvaluationBasis {
        self.score_evaluation_basis
    }
}
impl SetupTilingScore {
    pub fn continuation_available(&self) -> bool {
        self.continuation_available
    }
}
impl SetupTilingScore {
    pub fn continuation_available_complete(&self) -> bool {
        self.continuation_available_complete
    }
}
impl SetupTilingScore {
    pub fn build_variants(&self) -> &[SetupBuildScore] {
        &self.build_variants
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SetupFamilyScore {
    family_id: SetupFamilyId,
    build_coverage_probability: ProbabilityValue,
    post_pc_probability: ProbabilityValue,
    expected_score: f64,
    expected_attack: f64,
    total_solution_count: usize,
    score_evaluation_trace_count: usize,
    score_evaluation_complete: bool,
    score_evaluation_basis: ScoreEvaluationBasis,
    continuation_available: bool,
    continuation_available_complete: bool,
    tiling_variants: Vec<SetupTilingScore>,
}

impl SetupFamilyScore {
    pub(super) fn new(
        family_id: SetupFamilyId,
        totals: SetupScoreTotals,
        tiling_variants: Vec<SetupTilingScore>,
    ) -> Self {
        Self {
            family_id,
            build_coverage_probability: totals.coverage_probability,
            post_pc_probability: totals.post_pc_probability,
            expected_score: totals.expected_score,
            expected_attack: totals.expected_attack,
            total_solution_count: totals.total_solution_count,
            score_evaluation_trace_count: totals.score_evaluation_trace_count,
            score_evaluation_complete: totals.score_evaluation_complete,
            score_evaluation_basis: totals.score_evaluation_basis,
            continuation_available: totals.continuation_available,
            continuation_available_complete: totals.continuation_available_complete,
            tiling_variants,
        }
    }
}
impl SetupFamilyScore {
    pub fn family_id(&self) -> SetupFamilyId {
        self.family_id
    }
}
impl SetupFamilyScore {
    pub fn build_coverage_probability(&self) -> ProbabilityValue {
        self.build_coverage_probability
    }
}
impl SetupFamilyScore {
    pub fn post_pc_probability(&self) -> ProbabilityValue {
        self.post_pc_probability
    }
}
impl SetupFamilyScore {
    pub fn expected_score(&self) -> f64 {
        self.expected_score
    }
}
impl SetupFamilyScore {
    pub fn expected_attack(&self) -> f64 {
        self.expected_attack
    }
}
impl SetupFamilyScore {
    pub fn total_solution_count(&self) -> usize {
        self.total_solution_count
    }
}
impl SetupFamilyScore {
    pub fn score_evaluation_trace_count(&self) -> usize {
        self.score_evaluation_trace_count
    }
}
impl SetupFamilyScore {
    pub fn score_evaluation_complete(&self) -> bool {
        self.score_evaluation_complete
    }
}
impl SetupFamilyScore {
    pub fn score_evaluation_basis(&self) -> ScoreEvaluationBasis {
        self.score_evaluation_basis
    }
}
impl SetupFamilyScore {
    pub fn continuation_available(&self) -> bool {
        self.continuation_available
    }
}
impl SetupFamilyScore {
    pub fn continuation_available_complete(&self) -> bool {
        self.continuation_available_complete
    }
}
impl SetupFamilyScore {
    pub fn tiling_variants(&self) -> &[SetupTilingScore] {
        &self.tiling_variants
    }
}
