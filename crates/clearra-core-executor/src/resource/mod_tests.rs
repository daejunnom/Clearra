use super::*;

#[test]
fn frontier_budget_exceeded_marks_truncated() {
    let mut tracker = BatchResourceTracker::new(ResourceBudget::default().with_frontier_states(2));

    assert!(!tracker.observe_frontier_states(3));

    assert!(tracker.report().truncated);
    assert_eq!(
        tracker.report().truncation_reason,
        Some(ResourceTruncationReason::FrontierBudgetExceeded)
    );
    assert_eq!(tracker.report().peak_frontier_states, 3);
}

#[test]
fn candidate_budget_exceeded_marks_truncated() {
    let mut tracker = BatchResourceTracker::new(ResourceBudget::default().with_candidate_rows(1));

    assert!(!tracker.observe_candidate_rows(2));

    assert!(tracker.report().truncated);
    assert_eq!(
        tracker.report().truncation_reason,
        Some(ResourceTruncationReason::CandidateBudgetExceeded)
    );
    assert_eq!(tracker.report().peak_candidate_rows, 2);
}

#[test]
fn coverage_budget_exceeded_probability_complete_false() {
    let mut tracker = BatchResourceTracker::new(ResourceBudget::default().with_coverage_rows(2));

    assert!(!tracker.observe_coverage_rows(3));

    assert!(tracker.report().truncated);
    assert!(!tracker.report().probability_complete);
    assert_eq!(tracker.report().coverage_rows_emitted, 3);
}

#[test]
fn resource_truncation_probability_complete_false() {
    coverage_budget_exceeded_probability_complete_false();
}

#[test]
fn observed_truncated_universe_not_renormalized() {
    let mut tracker = BatchResourceTracker::new(ResourceBudget::default());

    tracker.mark_observed_universe_truncated();

    assert!(tracker.report().truncated);
    assert!(!tracker.report().probability_complete);
    assert_eq!(
        tracker.report().truncation_reason,
        Some(ResourceTruncationReason::ObservedUniverseTruncated)
    );
}
