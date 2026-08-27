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
fn attack_cannot_change_score_tie_selection() {
    let weights = WeightedPatternSet::new(vec![weight(1.0)]).expect("weights");
    let required = bitset(1, &[0]);
    let candidates = vec![
        ScoredCoverageCandidate::new(0, bitset(1, &[0]), 200, 0),
        ScoredCoverageCandidate::new(1, bitset(1, &[0]), 200, 999),
    ];

    let result = MaxScoreCover::select(
        &candidates,
        &required,
        &weights,
        MaxScoreCoverPolicy::default(),
    )
    .expect("selection");

    assert_eq!(result.selected_candidate_ids(), &[0]);
    assert_eq!(result.expected_score(), 200.0);
    assert_eq!(result.expected_attack(), 0.0);
}

#[test]
fn score_optimal_portfolios_preserve_all_original_candidate_identities() {
    let weights = WeightedPatternSet::uniform(3).expect("weights");
    let required = bitset(3, &[0, 1, 2]);
    let candidates = vec![
        ScoredCoverageCandidate::new(10, bitset(3, &[0, 1]), 100, 999),
        ScoredCoverageCandidate::new(20, bitset(3, &[0]), 100, 0),
        ScoredCoverageCandidate::new(30, bitset(3, &[1, 2]), 100, 1),
    ];
    let mut portfolios = MaxScoreCover::portfolio_enumerator(
        &candidates,
        &required,
        &weights,
        MaxScoreCoverPolicy::default(),
    )
    .expect("portfolio enumerator");

    let page = portfolios.next_page(10, 10).expect("all alternatives");
    assert_eq!(page.optimal_cardinality(), 2);
    assert_eq!(
        page.portfolios()
            .iter()
            .map(|portfolio| portfolio.candidate_ids().to_vec())
            .collect::<Vec<_>>(),
        vec![vec![10, 30], vec![20, 30]]
    );
    assert_eq!(page.total_alternative_count_decimal(), Some("2"));
    assert!(page.enumeration_complete());
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
    assert_eq!(
        MaxScoreCoverPolicy::new(1.0, 0.001),
        Err(MaxScoreCoverPolicyError::AttackWeightNotAllowed)
    );
}

#[test]
fn matrix_uses_canonical_trace_attack_only_as_information() {
    let weights = WeightedPatternSet::uniform(1).expect("weights");
    let required = bitset(1, &[0]);
    let matrix = MaterializedScoreMatrix::new(
        1,
        vec![
            MaterializedScoreCell::new(7, pattern(0), "trace-z", 500, 99, "exact"),
            MaterializedScoreCell::new(7, pattern(0), "trace-a", 500, 1, "exact"),
        ],
        "test",
        "exact",
        true,
    );

    let result =
        MaxScoreCover::select_matrix(&matrix, &required, &weights, MaxScoreCoverPolicy::default())
            .expect("matrix selection");

    assert_eq!(result.selected_candidate_ids(), &[7]);
    assert_eq!(
        result.pattern_contributions()[0].trace_identity(),
        Some("trace-a")
    );
    assert_eq!(result.pattern_contributions()[0].attack(), 1);
}
