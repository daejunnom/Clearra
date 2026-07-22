use clearra_core_domain::{
    ids::setup_id::{BuildVariantId, SetupFamilyId, TilingVariantId},
    probability::probability_value::ProbabilityValue,
};
use clearra_coverage::pattern::{
    pattern_bitset::PatternBitSet, pattern_id::PatternId, weighted_pattern_set::WeightedPatternSet,
};
use clearra_objectives::max_score::MaxScoreCoverPolicy;
use clearra_pc_graph::request::PcCompletionGoal;

use crate::evaluate::{
    PostPcEvaluation, PostPcEvaluationSummary, PostPcScoreSummary, ScoreEvaluationBasis,
};

use super::{SetupBuildScoreInput, SetupScoreAggregation, SetupScoreAggregationError};

fn weight(value: f64) -> ProbabilityValue {
    ProbabilityValue::new(value).expect("valid weight")
}

fn bitset(pattern_count: usize, patterns: &[usize]) -> PatternBitSet {
    PatternBitSet::from_patterns(pattern_count, patterns.iter().copied().map(PatternId::new))
        .expect("coverage")
}

fn successful_post_pc(score: u64, attack: u32, solutions: usize) -> PostPcEvaluation {
    PostPcEvaluation::Evaluated(PostPcEvaluationSummary::new(
        true,
        PcCompletionGoal::ClearToEmpty,
        2,
        solutions,
        solutions,
        1,
        true,
        true,
        PostPcScoreSummary::new(
            "test-score",
            score,
            attack,
            1,
            false,
            ScoreEvaluationBasis::RetainedTraces,
        ),
    ))
}

#[test]
fn setup_score_aggregation_preserves_family_tiling_build_layers() {
    let family_id = SetupFamilyId::new(1);
    let weights = WeightedPatternSet::new(vec![weight(0.4), weight(0.6)]).expect("weights");
    let builds = vec![
        SetupBuildScoreInput::new(
            family_id,
            TilingVariantId::new(10),
            BuildVariantId::new(100),
            bitset(2, &[0]),
            successful_post_pc(100, 1, 3),
        ),
        SetupBuildScoreInput::new(
            family_id,
            TilingVariantId::new(10),
            BuildVariantId::new(101),
            bitset(2, &[0]),
            successful_post_pc(200, 2, 5),
        ),
        SetupBuildScoreInput::new(
            family_id,
            TilingVariantId::new(11),
            BuildVariantId::new(102),
            bitset(2, &[1]),
            successful_post_pc(50, 10, 7),
        ),
    ];

    let score = SetupScoreAggregation::aggregate_family(
        family_id,
        &builds,
        &weights,
        MaxScoreCoverPolicy::default(),
    )
    .expect("aggregation");

    assert_eq!(score.family_id(), family_id);
    assert_eq!(score.build_coverage_probability().get(), 1.0);
    assert_eq!(score.post_pc_probability().get(), 1.0);
    assert_eq!(score.expected_score(), 110.0);
    assert_eq!(score.expected_attack(), 6.8);
    assert_eq!(score.total_solution_count(), 15);
    assert_eq!(score.score_evaluation_trace_count(), 3);
    assert!(!score.score_evaluation_complete());
    assert_eq!(
        score.score_evaluation_basis(),
        ScoreEvaluationBasis::RetainedTraces
    );
    assert!(score.continuation_available());
    assert_eq!(score.tiling_variants().len(), 2);
    assert_eq!(
        score.tiling_variants()[0].tiling_variant_id(),
        TilingVariantId::new(10)
    );
    assert_eq!(score.tiling_variants()[0].build_variants().len(), 2);
    assert_eq!(
        score.tiling_variants()[0].score_evaluation_basis(),
        ScoreEvaluationBasis::RetainedTraces
    );
    assert_eq!(
        score.tiling_variants()[1].tiling_variant_id(),
        TilingVariantId::new(11)
    );
    assert_eq!(score.tiling_variants()[1].build_variants().len(), 1);
}

#[test]
fn setup_score_aggregation_does_not_double_count_duplicate_pattern_probability() {
    let family_id = SetupFamilyId::new(1);
    let weights = WeightedPatternSet::new(vec![weight(0.4), weight(0.6)]).expect("weights");
    let builds = vec![
        SetupBuildScoreInput::new(
            family_id,
            TilingVariantId::new(10),
            BuildVariantId::new(100),
            bitset(2, &[0]),
            successful_post_pc(100, 1, 1),
        ),
        SetupBuildScoreInput::new(
            family_id,
            TilingVariantId::new(10),
            BuildVariantId::new(101),
            bitset(2, &[0]),
            successful_post_pc(200, 2, 1),
        ),
    ];

    let score = SetupScoreAggregation::aggregate_family(
        family_id,
        &builds,
        &weights,
        MaxScoreCoverPolicy::default(),
    )
    .expect("aggregation");

    assert_eq!(score.build_coverage_probability().get(), 0.4);
    assert_eq!(score.post_pc_probability().get(), 0.4);
    assert_eq!(score.expected_score(), 80.0);
    assert_eq!(score.expected_attack(), 0.8);
}

#[test]
fn setup_score_aggregation_discloses_sample_basis_when_any_build_is_sampled() {
    let family_id = SetupFamilyId::new(1);
    let weights = WeightedPatternSet::new(vec![weight(0.5), weight(0.5)]).expect("weights");
    let builds = vec![
        SetupBuildScoreInput::new(
            family_id,
            TilingVariantId::new(10),
            BuildVariantId::new(100),
            bitset(2, &[0]),
            PostPcEvaluation::Evaluated(PostPcEvaluationSummary::new(
                true,
                PcCompletionGoal::ClearToEmpty,
                2,
                1,
                1,
                1,
                true,
                true,
                PostPcScoreSummary::new(
                    "test-score",
                    100,
                    1,
                    1,
                    true,
                    ScoreEvaluationBasis::AllTraces,
                ),
            )),
        ),
        SetupBuildScoreInput::new(
            family_id,
            TilingVariantId::new(11),
            BuildVariantId::new(101),
            bitset(2, &[1]),
            PostPcEvaluation::Evaluated(PostPcEvaluationSummary::new(
                true,
                PcCompletionGoal::ClearToEmpty,
                2,
                100,
                100,
                4,
                true,
                true,
                PostPcScoreSummary::new(
                    "test-score",
                    200,
                    2,
                    4,
                    false,
                    ScoreEvaluationBasis::Sample,
                ),
            )),
        ),
    ];

    let score = SetupScoreAggregation::aggregate_family(
        family_id,
        &builds,
        &weights,
        MaxScoreCoverPolicy::default(),
    )
    .expect("aggregation");

    assert_eq!(score.score_evaluation_trace_count(), 5);
    assert!(!score.score_evaluation_complete());
    assert_eq!(score.score_evaluation_basis(), ScoreEvaluationBasis::Sample);
    assert_eq!(
        score.tiling_variants()[1].score_evaluation_basis(),
        ScoreEvaluationBasis::Sample
    );
    assert_eq!(
        score.tiling_variants()[1].build_variants()[0].score_evaluation_basis(),
        ScoreEvaluationBasis::Sample
    );
}

#[test]
fn setup_score_aggregation_rejects_mismatched_family() {
    let weights = WeightedPatternSet::uniform(1).expect("weights");
    let builds = vec![SetupBuildScoreInput::new(
        SetupFamilyId::new(2),
        TilingVariantId::new(10),
        BuildVariantId::new(100),
        bitset(1, &[0]),
        successful_post_pc(1, 0, 1),
    )];

    assert_eq!(
        SetupScoreAggregation::aggregate_family(
            SetupFamilyId::new(1),
            &builds,
            &weights,
            MaxScoreCoverPolicy::default()
        ),
        Err(SetupScoreAggregationError::FamilyMismatch {
            expected: SetupFamilyId::new(1),
            actual: SetupFamilyId::new(2)
        })
    );
}
