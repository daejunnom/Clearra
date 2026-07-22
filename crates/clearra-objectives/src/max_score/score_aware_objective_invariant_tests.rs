use super::*;

#[test]
fn score_does_not_modify_coverage_probability() {
    let probability = ProbabilityValue::new(0.75).expect("probability");
    let report = ScoreAwareObjectiveInvariantReport::new(probability, probability);

    assert_eq!(report.coverage_probability_before_scoring(), probability);
    assert_eq!(report.coverage_probability_after_scoring(), probability);
    assert!(report.score_does_not_modify_coverage_probability());
    assert!(report.objective_score_does_not_modify_coverage_probability());
    assert!(report.score_probability_no_double_count());
}

#[test]
fn score_objective_probability_change_is_visible() {
    let before = ProbabilityValue::new(0.75).expect("before");
    let after = ProbabilityValue::new(0.5).expect("after");
    let report = ScoreAwareObjectiveInvariantReport::new(before, after);

    assert!(!report.score_does_not_modify_coverage_probability());
    assert!(!report.objective_score_does_not_modify_coverage_probability());
    assert!(report.score_probability_no_double_count());
}
