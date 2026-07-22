use clearra_core_domain::{
    ids::setup_id::{BuildVariantId, SetupFamilyId, TilingVariantId},
    probability::probability_value::ProbabilityValue,
};
use clearra_coverage::{
    pattern::weighted_pattern_set::WeightedPatternSet,
    probability::union_probability::union_probability,
};

use crate::evaluate::{PostPcEvaluation, ScoreEvaluationBasis};

use super::{
    setup_score_aggregation::{validate_pattern_universe, SetupScoreAggregationError},
    SetupBuildScoreInput,
};

#[derive(Clone, Debug, PartialEq)]
pub struct SetupBuildScore {
    family_id: SetupFamilyId,
    tiling_variant_id: TilingVariantId,
    build_variant_id: BuildVariantId,
    coverage_probability: ProbabilityValue,
    post_pc_probability: ProbabilityValue,
    expected_score: f64,
    expected_attack: f64,
    best_score: u64,
    best_attack: u32,
    total_solution_count: usize,
    score_evaluation_trace_count: usize,
    score_evaluation_complete: bool,
    score_evaluation_basis: ScoreEvaluationBasis,
    continuation_available: bool,
    continuation_available_complete: bool,
    post_pc: PostPcEvaluation,
}

impl SetupBuildScore {
    pub(super) fn from_input(
        input: &SetupBuildScoreInput,
        weights: &WeightedPatternSet,
    ) -> Result<Self, SetupScoreAggregationError> {
        validate_pattern_universe(input.coverage(), weights)?;
        let coverage_probability = union_probability(input.coverage(), weights)
            .map_err(SetupScoreAggregationError::Probability)?;
        let post_pc_probability = if input.post_pc().solution_found() {
            coverage_probability
        } else {
            ProbabilityValue::ZERO
        };
        let summary = input.post_pc().summary();
        let best_score = summary.map_or(0, |summary| summary.score().best_score());
        let best_attack = summary.map_or(0, |summary| summary.score().best_attack());
        let total_solution_count = summary.map_or(0, |summary| summary.total_solution_count());
        let score_evaluation_trace_count =
            summary.map_or(0, |summary| summary.score_evaluation_trace_count());
        let score_evaluation_complete =
            summary.is_some_and(|summary| summary.score_evaluation_complete());
        let score_evaluation_basis = summary
            .map_or(ScoreEvaluationBasis::RetainedTraces, |summary| {
                summary.score_evaluation_basis()
            });
        let continuation_available =
            summary.is_some_and(|summary| summary.continuation_available());
        let continuation_available_complete =
            summary.is_some_and(|summary| summary.continuation_available_complete());

        Ok(Self {
            family_id: input.family_id(),
            tiling_variant_id: input.tiling_variant_id(),
            build_variant_id: input.build_variant_id(),
            coverage_probability,
            post_pc_probability,
            expected_score: coverage_probability.get() * best_score as f64,
            expected_attack: coverage_probability.get() * best_attack as f64,
            best_score,
            best_attack,
            total_solution_count,
            score_evaluation_trace_count,
            score_evaluation_complete,
            score_evaluation_basis,
            continuation_available,
            continuation_available_complete,
            post_pc: input.post_pc().clone(),
        })
    }
}
impl SetupBuildScore {
    pub fn family_id(&self) -> SetupFamilyId {
        self.family_id
    }
}
impl SetupBuildScore {
    pub fn tiling_variant_id(&self) -> TilingVariantId {
        self.tiling_variant_id
    }
}
impl SetupBuildScore {
    pub fn build_variant_id(&self) -> BuildVariantId {
        self.build_variant_id
    }
}
impl SetupBuildScore {
    pub fn coverage_probability(&self) -> ProbabilityValue {
        self.coverage_probability
    }
}
impl SetupBuildScore {
    pub fn post_pc_probability(&self) -> ProbabilityValue {
        self.post_pc_probability
    }
}
impl SetupBuildScore {
    pub fn expected_score(&self) -> f64 {
        self.expected_score
    }
}
impl SetupBuildScore {
    pub fn expected_attack(&self) -> f64 {
        self.expected_attack
    }
}
impl SetupBuildScore {
    pub fn best_score(&self) -> u64 {
        self.best_score
    }
}
impl SetupBuildScore {
    pub fn best_attack(&self) -> u32 {
        self.best_attack
    }
}
impl SetupBuildScore {
    pub fn total_solution_count(&self) -> usize {
        self.total_solution_count
    }
}
impl SetupBuildScore {
    pub fn score_evaluation_trace_count(&self) -> usize {
        self.score_evaluation_trace_count
    }
}
impl SetupBuildScore {
    pub fn score_evaluation_complete(&self) -> bool {
        self.score_evaluation_complete
    }
}
impl SetupBuildScore {
    pub fn score_evaluation_basis(&self) -> ScoreEvaluationBasis {
        self.score_evaluation_basis
    }
}
impl SetupBuildScore {
    pub fn continuation_available(&self) -> bool {
        self.continuation_available
    }
}
impl SetupBuildScore {
    pub fn continuation_available_complete(&self) -> bool {
        self.continuation_available_complete
    }
}
impl SetupBuildScore {
    pub fn post_pc(&self) -> &PostPcEvaluation {
        &self.post_pc
    }
}
