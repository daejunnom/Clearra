use clearra_scoring::{
    model::{CandidateScoreStats, MaxScoreBasis, PatternScoreContribution, ScoreExpectationReport},
    profile::{ScoreAccuracy, ScoreEvaluationScope},
};

#[test]
fn candidate_score_stats_distinguishes_retained_sample_and_universe_expected() {
    let retained = CandidateScoreStats::retained_sample(7, 1200);

    assert_eq!(
        retained.evaluation_scope(),
        ScoreEvaluationScope::RetainedTraceSample
    );
    assert_eq!(retained.conditional_average_score(), Some(1200));
    assert_eq!(retained.unconditional_expected_score(), None);
    assert_eq!(retained.score_accuracy(), ScoreAccuracy::TraceSampleOnly);

    let universe = CandidateScoreStats::universe_expected(7, 10, 900);

    assert_eq!(
        universe.evaluation_scope(),
        ScoreEvaluationScope::FullPatternUniverseExpected
    );
    assert_eq!(universe.covered_pattern_count(), 10);
    assert_eq!(universe.unconditional_expected_score(), Some(900));
}

#[test]
fn score_expectation_report_keeps_retained_average_separate_from_expected_score() {
    let retained = ScoreExpectationReport::retained_trace_average(500);

    assert_eq!(retained.retained_trace_average_score(), Some(500));
    assert_eq!(retained.unconditional_expected_score(), None);

    let full = ScoreExpectationReport::full_universe(800, 320);

    assert_eq!(full.retained_trace_average_score(), None);
    assert_eq!(full.covered_pattern_conditional_average_score(), Some(800));
    assert_eq!(full.unconditional_expected_score(), Some(320));
}

#[test]
fn retained_trace_average_is_not_unconditional_expected_score() {
    let retained = ScoreExpectationReport::retained_trace_average(500);
    let universe = ScoreExpectationReport::full_universe(800, 320);

    assert_eq!(retained.retained_trace_average_score(), Some(500));
    assert_eq!(retained.unconditional_expected_score(), None);
    assert_eq!(universe.retained_trace_average_score(), None);
    assert_eq!(universe.unconditional_expected_score(), Some(320));
}

#[test]
fn max_score_basis_keeps_best_score_by_pattern() {
    let basis = MaxScoreBasis::new(vec![
        PatternScoreContribution::new(0, 1, 100, 2),
        PatternScoreContribution::new(1, 3, 200, 4),
    ]);

    assert_eq!(basis.best_score_by_pattern_count(), 2);
    assert_eq!(basis.best_score_by_pattern()[1].candidate_id(), 3);
}
