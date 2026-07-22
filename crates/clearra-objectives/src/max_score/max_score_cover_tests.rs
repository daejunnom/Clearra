use clearra_core_domain::probability::probability_value::ProbabilityValue;
use clearra_coverage::pattern::pattern_id::PatternId;

use crate::max_score::MaxScoreCoverPolicyError;

use super::*;

fn pattern(index: usize) -> PatternId {
    PatternId::new(index)
}

fn weight(value: f64) -> ProbabilityValue {
    ProbabilityValue::new(value).expect("valid probability")
}

fn bitset(pattern_count: usize, patterns: &[usize]) -> PatternBitSet {
    PatternBitSet::from_patterns(pattern_count, patterns.iter().copied().map(pattern))
        .expect("valid bitset")
}

#[test]
fn score_aware_cover_uses_pattern_union_probability_not_variant_sum() {
    let weights = WeightedPatternSet::new(vec![weight(0.4), weight(0.6)]).expect("weights");
    let required = bitset(2, &[0]);
    let candidates = vec![
        ScoredCoverageCandidate::new(7, bitset(2, &[0]), 100, 1),
        ScoredCoverageCandidate::new(8, bitset(2, &[0]), 200, 4),
    ];

    let result = MaxScoreCover::select(
        &candidates,
        &required,
        &weights,
        MaxScoreCoverPolicy::default(),
    )
    .expect("max-score selection");

    assert_eq!(result.selected_candidate_ids(), &[8]);
    assert_eq!(result.covered_probability().get(), 0.4);
    assert_eq!(result.expected_score(), 80.0);
    assert_eq!(result.expected_attack(), 1.6);
    assert!(result.complete());
}

#[test]
fn max_score_cover_does_not_double_count_probability() {
    score_aware_cover_uses_pattern_union_probability_not_variant_sum();
}

#[test]
fn score_aware_cover_selects_best_candidate_per_pattern() {
    let weights = WeightedPatternSet::new(vec![weight(0.25), weight(0.75)]).expect("weights");
    let required = bitset(2, &[0, 1]);
    let candidates = vec![
        ScoredCoverageCandidate::new(0, bitset(2, &[0]), 100, 1),
        ScoredCoverageCandidate::new(1, bitset(2, &[0, 1]), 50, 10),
    ];

    let result = MaxScoreCover::select(
        &candidates,
        &required,
        &weights,
        MaxScoreCoverPolicy::default(),
    )
    .expect("max-score selection");

    assert_eq!(result.selected_candidate_ids(), &[0, 1]);
    assert_eq!(result.covered_probability().get(), 1.0);
    assert_eq!(result.expected_score(), 62.5);
    assert_eq!(result.expected_attack(), 7.75);
    assert!(result.complete());
    assert_eq!(result.pattern_contributions().len(), 2);
    assert_eq!(result.best_score_by_pattern_count(), 2);
    assert_eq!(result.best_score_by_pattern()[0].candidate_id(), 0);
    assert_eq!(result.best_score_by_pattern()[1].candidate_id(), 1);
}

#[test]
fn max_score_cover_uses_best_score_by_pattern() {
    let weights = WeightedPatternSet::new(vec![weight(0.25), weight(0.75)]).expect("weights");
    let required = bitset(2, &[0, 1]);
    let candidates = vec![
        ScoredCoverageCandidate::new(0, bitset(2, &[0]), 100, 1),
        ScoredCoverageCandidate::new(1, bitset(2, &[0]), 250, 1),
        ScoredCoverageCandidate::new(2, bitset(2, &[1]), 50, 10),
    ];

    let result = MaxScoreCover::select(
        &candidates,
        &required,
        &weights,
        MaxScoreCoverPolicy::default(),
    )
    .expect("max-score selection");

    assert_eq!(result.selected_candidate_ids(), &[1, 2]);
    assert_eq!(result.best_score_by_pattern_count(), 2);
    assert_eq!(result.best_score_by_pattern()[0].candidate_id(), 1);
    assert_eq!(result.best_score_by_pattern()[1].candidate_id(), 2);
    assert_eq!(result.covered_probability().get(), 1.0);
}

#[test]
fn max_score_cover_selects_best_score_by_pattern() {
    let weights = WeightedPatternSet::new(vec![weight(0.25), weight(0.75)]).expect("weights");
    let required = bitset(2, &[0, 1]);
    let candidates = vec![
        ScoredCoverageCandidate::new(0, bitset(2, &[0]), 100, 1),
        ScoredCoverageCandidate::new(1, bitset(2, &[0]), 250, 1),
        ScoredCoverageCandidate::new(2, bitset(2, &[1]), 50, 10),
    ];

    let result = MaxScoreCover::select(
        &candidates,
        &required,
        &weights,
        MaxScoreCoverPolicy::default(),
    )
    .expect("max-score selection");

    assert_eq!(result.best_score_by_pattern()[0].candidate_id(), 1);
    assert_eq!(result.best_score_by_pattern()[1].candidate_id(), 2);
    assert_eq!(result.covered_probability().get(), 1.0);
}

#[test]
fn score_aware_cover_can_rank_by_attack_expectation() {
    let weights = WeightedPatternSet::new(vec![weight(1.0)]).expect("weights");
    let required = bitset(1, &[0]);
    let candidates = vec![
        ScoredCoverageCandidate::new(0, bitset(1, &[0]), 200, 0),
        ScoredCoverageCandidate::new(1, bitset(1, &[0]), 20, 8),
    ];
    let policy = MaxScoreCoverPolicy::new(0.0, 1.0).expect("attack policy");

    let result =
        MaxScoreCover::select(&candidates, &required, &weights, policy).expect("selection");

    assert_eq!(result.selected_candidate_ids(), &[1]);
    assert_eq!(result.expected_score(), 20.0);
    assert_eq!(result.expected_attack(), 8.0);
}

#[test]
fn score_aware_cover_reports_incomplete_required_patterns() {
    let weights = WeightedPatternSet::new(vec![weight(0.5), weight(0.5)]).expect("weights");
    let required = bitset(2, &[0, 1]);
    let candidates = vec![ScoredCoverageCandidate::new(3, bitset(2, &[0]), 80, 2)];

    let result = MaxScoreCover::select(
        &candidates,
        &required,
        &weights,
        MaxScoreCoverPolicy::default(),
    )
    .expect("max-score selection");

    assert_eq!(result.selected_candidate_ids(), &[3]);
    assert_eq!(result.covered_probability().get(), 0.5);
    assert_eq!(result.expected_score(), 40.0);
    assert!(!result.complete());
}

#[test]
fn score_aware_cover_rejects_pattern_universe_mismatch() {
    let weights = WeightedPatternSet::uniform(2).expect("weights");
    let required = bitset(2, &[0]);
    let candidates = vec![ScoredCoverageCandidate::new(0, bitset(3, &[0]), 10, 0)];

    assert_eq!(
        MaxScoreCover::select(
            &candidates,
            &required,
            &weights,
            MaxScoreCoverPolicy::default()
        ),
        Err(MaxScoreCoverError::CandidatePatternUniverseMismatch {
            expected: 2,
            actual: 3
        })
    );
}

#[test]
fn max_score_policy_rejects_invalid_weights() {
    assert_eq!(
        MaxScoreCoverPolicy::new(f64::NAN, 0.0),
        Err(MaxScoreCoverPolicyError::NonFiniteWeight)
    );
    assert_eq!(
        MaxScoreCoverPolicy::new(-1.0, 0.0),
        Err(MaxScoreCoverPolicyError::NegativeWeight)
    );
    assert_eq!(
        MaxScoreCoverPolicy::new(0.0, 0.0),
        Err(MaxScoreCoverPolicyError::EmptyObjective)
    );
}
