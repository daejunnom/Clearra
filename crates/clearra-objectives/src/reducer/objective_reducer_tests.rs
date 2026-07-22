use clearra_core_domain::probability::probability_value::ProbabilityValue;
use clearra_coverage::pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId};

use super::*;

fn weight(value: f64) -> ProbabilityValue {
    ProbabilityValue::new(value).expect("valid probability")
}

fn bitset(pattern_count: usize, patterns: &[usize]) -> PatternBitSet {
    PatternBitSet::from_patterns(pattern_count, patterns.iter().copied().map(PatternId::new))
        .expect("patterns")
}

fn identity() -> ObjectiveCoverageIdentity {
    ObjectiveCoverageIdentity::new(
        CoverageRowKind::Build,
        11,
        PatternUniverseId::new(1),
        PatternWeightModelId::new(7),
    )
}

#[test]
fn objective_reducer_uses_or_probability_not_variant_sum() {
    let candidates = vec![
        ObjectiveCandidate::new(1, "trk1:a", bitset(2, &[0]), 10, 1),
        ObjectiveCandidate::new(2, "trk1:b", bitset(2, &[0]), 20, 1),
    ];
    let required = bitset(2, &[0]);
    let weights = WeightedPatternSet::new(vec![weight(0.8), weight(0.2)]).expect("weights");

    let result = ObjectiveReducer::reduce(
        &candidates,
        &required,
        &weights,
        ObjectiveCountInput::new(2, 2, true, false),
        identity(),
        MaxScoreCoverPolicy::default(),
    )
    .expect("result");

    assert_eq!(result.coverage().probability().get(), 0.8);
    assert_eq!(result.all_candidate_ids(), &[1, 2]);
}

#[test]
fn minimum_cover_works_on_coverage_matrix() {
    let candidates = vec![
        ObjectiveCandidate::new(1, "trk1:a", bitset(3, &[0, 1]), 10, 1),
        ObjectiveCandidate::new(2, "trk1:b", bitset(3, &[2]), 20, 1),
    ];
    let required = bitset(3, &[0, 1, 2]);
    let weights = WeightedPatternSet::uniform(3).expect("weights");

    let result = ObjectiveReducer::reduce(
        &candidates,
        &required,
        &weights,
        ObjectiveCountInput::new(2, 2, true, false),
        identity(),
        MaxScoreCoverPolicy::default(),
    )
    .expect("result");

    assert!(result.minimum_cover().is_complete());
    assert_eq!(result.minimum_cover().row_indices(), &[0, 1]);
}

#[test]
fn unique_result_uses_stable_canonical_key() {
    let candidates = vec![
        ObjectiveCandidate::new(1, "trk1:same", bitset(2, &[0]), 10, 1),
        ObjectiveCandidate::new(2, "trk1:same", bitset(2, &[1]), 20, 1),
        ObjectiveCandidate::new(3, "trk1:other", bitset(2, &[1]), 30, 1),
    ];
    let required = bitset(2, &[0, 1]);
    let weights = WeightedPatternSet::uniform(2).expect("weights");

    let result = ObjectiveReducer::reduce(
        &candidates,
        &required,
        &weights,
        ObjectiveCountInput::new(3, 3, true, false),
        identity(),
        MaxScoreCoverPolicy::default(),
    )
    .expect("result");

    assert_eq!(result.unique_candidate_ids(), &[1, 3]);
    assert_eq!(result.unique_result_count(), 2);
}

#[test]
fn retained_trace_count_is_separate_from_total_count() {
    let candidates = vec![ObjectiveCandidate::new(
        1,
        "trk1:sample",
        bitset(1, &[0]),
        10,
        1,
    )];
    let required = bitset(1, &[0]);
    let weights = WeightedPatternSet::uniform(1).expect("weights");

    let result = ObjectiveReducer::reduce(
        &candidates,
        &required,
        &weights,
        ObjectiveCountInput::new(12_000, 64, true, true),
        identity(),
        MaxScoreCoverPolicy::default(),
    )
    .expect("result");

    assert_eq!(result.total_solution_count(), 12_000);
    assert_eq!(result.retained_trace_count(), 64);
    assert!(result.count_complete());
    assert!(result.trace_retention_truncated());
}

#[test]
fn max_score_and_dominance_reducers_share_coverage_candidates() {
    let candidates = vec![
        ObjectiveCandidate::new(1, "trk1:weak", bitset(2, &[0]), 10, 1),
        ObjectiveCandidate::new(2, "trk1:strong", bitset(2, &[0, 1]), 100, 2),
    ];
    let required = bitset(2, &[0, 1]);
    let weights = WeightedPatternSet::uniform(2).expect("weights");

    let result = ObjectiveReducer::reduce(
        &candidates,
        &required,
        &weights,
        ObjectiveCountInput::new(2, 2, true, false),
        identity(),
        MaxScoreCoverPolicy::default(),
    )
    .expect("result");

    let max_score = result.max_score().expect("materialized score");
    assert_eq!(max_score.selected_candidate_ids(), &[2]);
    assert_eq!(max_score.best_score_by_pattern_count(), 2);
    assert_eq!(result.non_dominated_candidate_ids(), &[2]);
}

#[test]
fn max_score_objective_reports_best_score_by_pattern_without_changing_coverage_probability() {
    let candidates = vec![
        ObjectiveCandidate::new(1, "trk1:low", bitset(2, &[0]), 10, 1),
        ObjectiveCandidate::new(2, "trk1:high", bitset(2, &[0]), 30, 3),
        ObjectiveCandidate::new(3, "trk1:other", bitset(2, &[1]), 20, 2),
    ];
    let required = bitset(2, &[0, 1]);
    let weights = WeightedPatternSet::new(vec![weight(0.25), weight(0.75)]).expect("weights");

    let result = ObjectiveReducer::reduce(
        &candidates,
        &required,
        &weights,
        ObjectiveCountInput::new(3, 2, true, true),
        identity(),
        MaxScoreCoverPolicy::default(),
    )
    .expect("result");

    let max_score = result.max_score().expect("materialized score");
    assert_eq!(result.coverage().probability().get(), 1.0);
    assert_eq!(max_score.covered_probability().get(), 1.0);
    assert_eq!(max_score.best_score_by_pattern_count(), 2);
    assert_eq!(max_score.best_score_by_pattern()[0].candidate_id(), 2);
    assert_eq!(max_score.best_score_by_pattern()[1].candidate_id(), 3);
    assert_eq!(max_score.expected_score(), 22.5);
}

#[test]
fn score_does_not_change_coverage_probability() {
    let candidates = vec![
        ObjectiveCandidate::new(1, "trk1:low", bitset(2, &[0]), 10, 1),
        ObjectiveCandidate::new(2, "trk1:high", bitset(2, &[0]), 30, 3),
        ObjectiveCandidate::new(3, "trk1:other", bitset(2, &[1]), 20, 2),
    ];
    let required = bitset(2, &[0, 1]);
    let weights = WeightedPatternSet::new(vec![weight(0.25), weight(0.75)]).expect("weights");

    let result = ObjectiveReducer::reduce(
        &candidates,
        &required,
        &weights,
        ObjectiveCountInput::new(3, 2, true, true),
        identity(),
        MaxScoreCoverPolicy::default(),
    )
    .expect("result");

    assert_eq!(result.coverage().probability().get(), 1.0);
    let max_score = result.max_score().expect("materialized score");
    assert_eq!(max_score.covered_probability().get(), 1.0);
    assert_eq!(max_score.best_score_by_pattern_count(), 2);
}
