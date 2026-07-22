use super::*;

#[test]
fn search_execution_report_keeps_solution_count_trace_retention_and_candidates_separate() {
    let fields = vec![
        ("backend_requested".to_owned(), "gpu".to_owned()),
        (
            "backend_selected".to_owned(),
            "cpu-geometry-exact-cover".to_owned(),
        ),
        (
            "backend_fallback_reason".to_owned(),
            "gpu_feature_disabled".to_owned(),
        ),
        ("packing_candidate_count".to_owned(), "99".to_owned()),
        ("solution_found".to_owned(), "true".to_owned()),
        ("total_solution_count".to_owned(), "12".to_owned()),
        ("unique_solution_count".to_owned(), "8".to_owned()),
        ("retained_trace_count".to_owned(), "2".to_owned()),
        ("count_complete".to_owned(), "true".to_owned()),
        ("trace_retention_truncated".to_owned(), "true".to_owned()),
        (
            "trace_retention_reason".to_owned(),
            "retained_trace_limit".to_owned(),
        ),
        ("coverage_probability".to_owned(), "0.75".to_owned()),
        ("coverage_row_count".to_owned(), "4".to_owned()),
    ];

    let report = SearchExecutionReport::from_summary_fields(&fields, Vec::new());

    assert_eq!(
        report.backend_report().backend_fallback_reason(),
        "gpu_feature_disabled"
    );
    assert_eq!(report.packing_result().candidate_count(), 99);
    assert_eq!(report.objective_result().total_solution_count(), 12);
    assert_eq!(report.objective_result().retained_trace_count(), 2);
    assert!(report.objective_result().count_complete());
    assert!(report.replay_trace().trace_retention_truncated());
    assert_eq!(report.coverage_result().coverage_probability(), "0.75");
}

#[test]
fn packing_candidate_is_not_solution_before_buildup() {
    let fields = vec![
        ("packing_candidate_count".to_owned(), "3".to_owned()),
        ("build_variant_count".to_owned(), "1".to_owned()),
        ("solution_found".to_owned(), "false".to_owned()),
        ("total_solution_count".to_owned(), "0".to_owned()),
        ("retained_trace_count".to_owned(), "0".to_owned()),
    ];

    let report = SearchExecutionReport::from_summary_fields(&fields, Vec::new());

    assert_eq!(report.packing_result().candidate_count(), 3);
    assert!(!report.buildup_result().solution_found());
    assert_eq!(report.buildup_result().total_solution_count(), 0);
    assert_eq!(report.objective_result().total_solution_count(), 0);
}

#[test]
fn build_variant_view_exposes_score_event_basis_and_kick_evidence_count() {
    let view = BuildVariantView::new("bvk1:abc", "0.25")
        .with_score_event_basis("c-replay")
        .with_kick_evidence_count(2);

    assert_eq!(view.variant_id(), "bvk1:abc");
    assert_eq!(view.coverage_probability(), "0.25");
    assert_eq!(view.score_event_basis(), "c-replay");
    assert_eq!(view.kick_evidence_count(), 2);
}
