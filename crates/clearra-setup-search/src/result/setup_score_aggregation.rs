use std::collections::BTreeMap;

use clearra_core_domain::{
    ids::setup_id::{SetupFamilyId, TilingVariantId},
    probability::probability_value::ProbabilityValue,
};
use clearra_coverage::{
    pattern::{
        pattern_bitset::{PatternBitSet, PatternBitSetError},
        weighted_pattern_set::WeightedPatternSet,
    },
    probability::union_probability::{union_probability, UnionProbabilityError},
};
use clearra_objectives::max_score::{
    MaxScoreCover, MaxScoreCoverError, MaxScoreCoverPolicy, ScoredCoverageCandidate,
};

use crate::evaluate::ScoreEvaluationBasis;

use super::{SetupBuildScore, SetupBuildScoreInput, SetupFamilyScore, SetupTilingScore};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SetupScoreAggregation;

impl SetupScoreAggregation {
    pub fn aggregate_family(
        family_id: SetupFamilyId,
        builds: &[SetupBuildScoreInput],
        weights: &WeightedPatternSet,
        policy: MaxScoreCoverPolicy,
    ) -> Result<SetupFamilyScore, SetupScoreAggregationError> {
        for build in builds {
            if build.family_id() != family_id {
                return Err(SetupScoreAggregationError::FamilyMismatch {
                    expected: family_id,
                    actual: build.family_id(),
                });
            }
        }

        let build_scores = builds
            .iter()
            .map(|input| SetupBuildScore::from_input(input, weights))
            .collect::<Result<Vec<_>, _>>()?;
        let totals = score_totals(builds, weights, policy)?;
        let tiling_variants = tiling_scores(builds, &build_scores, weights, policy)?;

        Ok(SetupFamilyScore::new(family_id, totals, tiling_variants))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SetupScoreAggregationError {
    FamilyMismatch {
        expected: SetupFamilyId,
        actual: SetupFamilyId,
    },
    CoverageUniverseMismatch {
        expected: usize,
        actual: usize,
    },
    PatternBitSet(PatternBitSetError),
    Probability(UnionProbabilityError),
    MaxScore(MaxScoreCoverError),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SetupScoreTotals {
    pub(super) coverage_probability: ProbabilityValue,
    pub(super) post_pc_probability: ProbabilityValue,
    pub(super) expected_score: f64,
    pub(super) expected_attack: f64,
    pub(super) total_solution_count: usize,
    pub(super) score_evaluation_trace_count: usize,
    pub(super) score_evaluation_complete: bool,
    pub(super) score_evaluation_basis: ScoreEvaluationBasis,
    pub(super) continuation_available: bool,
    pub(super) continuation_available_complete: bool,
}

fn tiling_scores(
    builds: &[SetupBuildScoreInput],
    build_scores: &[SetupBuildScore],
    weights: &WeightedPatternSet,
    policy: MaxScoreCoverPolicy,
) -> Result<Vec<SetupTilingScore>, SetupScoreAggregationError> {
    let mut by_tiling = BTreeMap::<TilingVariantId, Vec<usize>>::new();
    for (index, build) in builds.iter().enumerate() {
        by_tiling
            .entry(build.tiling_variant_id())
            .or_default()
            .push(index);
    }

    by_tiling
        .into_iter()
        .map(|(tiling_variant_id, indices)| {
            let tiling_inputs = indices
                .iter()
                .map(|index| builds[*index].clone())
                .collect::<Vec<_>>();
            let tiling_build_scores = indices
                .iter()
                .map(|index| build_scores[*index].clone())
                .collect::<Vec<_>>();
            let totals = score_totals(&tiling_inputs, weights, policy)?;
            Ok(SetupTilingScore::new(
                tiling_variant_id,
                totals,
                tiling_build_scores,
            ))
        })
        .collect()
}

fn score_totals(
    builds: &[SetupBuildScoreInput],
    weights: &WeightedPatternSet,
    policy: MaxScoreCoverPolicy,
) -> Result<SetupScoreTotals, SetupScoreAggregationError> {
    let mut coverage_union = PatternBitSet::new(weights.len());
    let mut post_pc_union = PatternBitSet::new(weights.len());
    let mut score_candidates = Vec::new();
    let mut total_solution_count = 0;
    let mut score_evaluation_trace_count = 0;
    let mut score_evaluation_complete = true;
    let mut score_evaluation_basis = ScoreEvaluationBasis::AllTraces;
    let mut score_evaluation_seen = false;
    let mut continuation_available = false;
    let mut continuation_available_complete = true;
    let mut continuation_evaluation_seen = false;

    for build in builds {
        validate_pattern_universe(build.coverage(), weights)?;
        coverage_union
            .union_with(build.coverage())
            .map_err(SetupScoreAggregationError::PatternBitSet)?;

        let Some(summary) = build.post_pc().summary() else {
            continue;
        };
        total_solution_count += summary.total_solution_count();
        continuation_evaluation_seen = true;
        continuation_available |= summary.continuation_available();
        continuation_available_complete &= summary.continuation_available_complete();
        if score_summary_was_evaluated(summary.score()) {
            score_evaluation_seen = true;
            score_evaluation_trace_count += summary.score_evaluation_trace_count();
            score_evaluation_complete &= summary.score_evaluation_complete();
            score_evaluation_basis =
                score_evaluation_basis.combine(summary.score_evaluation_basis());
        }

        if summary.solution_found() {
            post_pc_union
                .union_with(build.coverage())
                .map_err(SetupScoreAggregationError::PatternBitSet)?;
            score_candidates.push(ScoredCoverageCandidate::new(
                build.build_variant_id().get() as usize,
                build.coverage().clone(),
                summary.score().best_score(),
                summary.score().best_attack(),
            ));
        }
    }

    let coverage_probability = union_probability(&coverage_union, weights)
        .map_err(SetupScoreAggregationError::Probability)?;
    let post_pc_probability = union_probability(&post_pc_union, weights)
        .map_err(SetupScoreAggregationError::Probability)?;
    let score_selection = MaxScoreCover::select(&score_candidates, &post_pc_union, weights, policy)
        .map_err(SetupScoreAggregationError::MaxScore)?;

    Ok(SetupScoreTotals {
        coverage_probability,
        post_pc_probability,
        expected_score: score_selection.expected_score(),
        expected_attack: score_selection.expected_attack(),
        total_solution_count,
        score_evaluation_trace_count,
        score_evaluation_complete: score_evaluation_seen && score_evaluation_complete,
        score_evaluation_basis: if score_evaluation_seen {
            score_evaluation_basis
        } else {
            ScoreEvaluationBasis::RetainedTraces
        },
        continuation_available,
        continuation_available_complete: continuation_available
            || (continuation_evaluation_seen && continuation_available_complete),
    })
}

fn score_summary_was_evaluated(summary: &crate::evaluate::PostPcScoreSummary) -> bool {
    !summary.profile_id().is_empty() || summary.score_evaluation_trace_count() > 0
}

pub(super) fn validate_pattern_universe(
    coverage: &PatternBitSet,
    weights: &WeightedPatternSet,
) -> Result<(), SetupScoreAggregationError> {
    if coverage.pattern_count() != weights.len() {
        return Err(SetupScoreAggregationError::CoverageUniverseMismatch {
            expected: weights.len(),
            actual: coverage.pattern_count(),
        });
    }
    Ok(())
}
