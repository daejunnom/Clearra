use super::*;

#[test]
fn search_execution_report_keeps_solution_count_trace_retention_and_candidates_separate() {
    let fields = vec![
        ("backend_requested".to_owned(), "gpu".to_owned()),
        ("search_output_policy".to_owned(), "summary".to_owned()),
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
        (
            "normalized_unique_solution_count".to_owned(),
            "8".to_owned(),
        ),
        ("solution_count_calculated".to_owned(), "true".to_owned()),
        ("solution_set_materialized".to_owned(), "true".to_owned()),
        (
            "solution_keys_materialized_count".to_owned(),
            "8".to_owned(),
        ),
        ("solution_keys_complete".to_owned(), "true".to_owned()),
        ("solution_page_available".to_owned(), "false".to_owned()),
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
    assert!(report
        .solution_set_availability()
        .solution_count_calculated());
    assert!(report
        .solution_set_availability()
        .solution_set_materialized());
    assert_eq!(
        report
            .solution_set_availability()
            .solution_keys_materialized_count(),
        8
    );
    assert!(report.solution_set_availability().solution_keys_complete());
    assert!(!report.solution_set_availability().solution_page_available());
    assert!(report.solution_set_availability().contract_valid());
    assert!(report.solution_set_availability().uses_explicit_contract());
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

#[test]
fn coverage_summary_preserves_not_calculated_as_availability_instead_of_zero() {
    let fields = vec![
        (
            "search_output_policy".to_owned(),
            "coverage-summary".to_owned(),
        ),
        (
            "unique_solution_count".to_owned(),
            "not-calculated".to_owned(),
        ),
        (
            "normalized_unique_solution_count".to_owned(),
            "not-calculated".to_owned(),
        ),
        (
            "normalized_solution_set_hash".to_owned(),
            "not-calculated".to_owned(),
        ),
        (
            "actual_normalized_solution_set_hash".to_owned(),
            "not-calculated".to_owned(),
        ),
        ("solution_count_calculated".to_owned(), "false".to_owned()),
        ("solution_set_materialized".to_owned(), "false".to_owned()),
        (
            "solution_keys_materialized_count".to_owned(),
            "0".to_owned(),
        ),
        ("solution_keys_complete".to_owned(), "false".to_owned()),
        ("solution_page_available".to_owned(), "false".to_owned()),
    ];

    let report = SearchExecutionReport::from_summary_fields(&fields, Vec::new());

    assert_eq!(report.objective_result().unique_solution_count(), 0);
    let availability = report.solution_set_availability();
    assert!(!availability.solution_count_calculated());
    assert!(!availability.solution_set_materialized());
    assert_eq!(availability.solution_keys_materialized_count(), 0);
    assert!(!availability.solution_keys_complete());
    assert!(!availability.solution_page_available());
    assert!(availability.contract_valid());
}

#[test]
fn calculated_zero_remains_distinct_from_not_calculated() {
    let fields = vec![
        ("search_output_policy".to_owned(), "summary".to_owned()),
        ("unique_solution_count".to_owned(), "0".to_owned()),
        (
            "normalized_unique_solution_count".to_owned(),
            "0".to_owned(),
        ),
        ("solution_count_calculated".to_owned(), "true".to_owned()),
        ("solution_set_materialized".to_owned(), "true".to_owned()),
        (
            "solution_keys_materialized_count".to_owned(),
            "0".to_owned(),
        ),
        ("solution_keys_complete".to_owned(), "true".to_owned()),
        ("solution_page_available".to_owned(), "false".to_owned()),
    ];

    let report = SearchExecutionReport::from_summary_fields(&fields, Vec::new());

    assert_eq!(report.objective_result().unique_solution_count(), 0);
    assert!(report
        .solution_set_availability()
        .solution_count_calculated());
    assert!(report
        .solution_set_availability()
        .solution_set_materialized());
    assert!(report.solution_set_availability().solution_keys_complete());
    assert!(report.solution_set_availability().contract_valid());
}

#[test]
fn legacy_numeric_counts_infer_calculated_and_materialized_availability() {
    let fields = vec![("unique_solution_count".to_owned(), "0".to_owned())];

    let report = SearchExecutionReport::from_summary_fields(&fields, Vec::new());

    let availability = report.solution_set_availability();
    assert!(availability.solution_count_calculated());
    assert!(availability.solution_set_materialized());
    assert!(availability.solution_keys_complete());
    assert!(availability.contract_valid());
    assert!(availability.uses_legacy_inference());
}

#[test]
fn coverage_summary_missing_or_malformed_markers_fail_closed_atomically() {
    let valid_fields = vec![
        (
            "search_output_policy".to_owned(),
            "coverage-summary".to_owned(),
        ),
        (
            "unique_solution_count".to_owned(),
            "not-calculated".to_owned(),
        ),
        (
            "normalized_unique_solution_count".to_owned(),
            "not-calculated".to_owned(),
        ),
        (
            "normalized_solution_set_hash".to_owned(),
            "not-calculated".to_owned(),
        ),
        (
            "actual_normalized_solution_set_hash".to_owned(),
            "not-calculated".to_owned(),
        ),
        ("solution_count_calculated".to_owned(), "false".to_owned()),
        ("solution_set_materialized".to_owned(), "false".to_owned()),
        (
            "solution_keys_materialized_count".to_owned(),
            "0".to_owned(),
        ),
        ("solution_keys_complete".to_owned(), "false".to_owned()),
        ("solution_page_available".to_owned(), "false".to_owned()),
    ];

    for key in [
        "unique_solution_count",
        "normalized_unique_solution_count",
        "normalized_solution_set_hash",
        "actual_normalized_solution_set_hash",
        "solution_count_calculated",
        "solution_set_materialized",
        "solution_keys_materialized_count",
        "solution_keys_complete",
        "solution_page_available",
    ] {
        let fields = valid_fields
            .iter()
            .filter(|(field_key, _)| field_key != key)
            .cloned()
            .collect::<Vec<_>>();
        let availability = SearchExecutionReport::from_summary_fields(&fields, Vec::new())
            .solution_set_availability()
            .clone();
        assert!(!availability.contract_valid(), "missing {key}");
        assert!(!availability.solution_count_calculated(), "missing {key}");
        assert!(!availability.solution_set_materialized(), "missing {key}");
        assert_eq!(
            availability.solution_keys_materialized_count(),
            0,
            "missing {key}"
        );
        assert!(!availability.solution_keys_complete(), "missing {key}");
        assert!(!availability.solution_page_available(), "missing {key}");
    }

    let malformed_cases = [
        ("unique_solution_count", "0"),
        ("normalized_unique_solution_count", "0"),
        ("normalized_solution_set_hash", "cts1:fake"),
        ("actual_normalized_solution_set_hash", "cts1:fake"),
        ("solution_count_calculated", "true"),
        ("solution_set_materialized", "true"),
        ("solution_keys_materialized_count", "1"),
        ("solution_keys_complete", "true"),
        ("solution_page_available", "true"),
    ];
    for (key, malformed_value) in malformed_cases {
        let mut fields = valid_fields.clone();
        fields
            .iter_mut()
            .find(|(field_key, _)| field_key == key)
            .expect("contract key")
            .1 = malformed_value.to_owned();
        let availability = SearchExecutionReport::from_summary_fields(&fields, Vec::new())
            .solution_set_availability()
            .clone();
        assert!(!availability.contract_valid(), "malformed {key}");
        assert!(!availability.solution_count_calculated(), "malformed {key}");
        assert!(!availability.solution_page_available(), "malformed {key}");
    }
}

#[test]
fn partial_explicit_contract_and_duplicate_markers_do_not_use_legacy_inference() {
    for fields in [
        vec![
            ("unique_solution_count".to_owned(), "3".to_owned()),
            ("solution_count_calculated".to_owned(), "true".to_owned()),
        ],
        vec![
            ("search_output_policy".to_owned(), "summary".to_owned()),
            ("unique_solution_count".to_owned(), "0".to_owned()),
            ("solution_count_calculated".to_owned(), "true".to_owned()),
            ("solution_count_calculated".to_owned(), "false".to_owned()),
            ("solution_set_materialized".to_owned(), "true".to_owned()),
            (
                "solution_keys_materialized_count".to_owned(),
                "0".to_owned(),
            ),
            ("solution_keys_complete".to_owned(), "true".to_owned()),
            ("solution_page_available".to_owned(), "false".to_owned()),
        ],
    ] {
        let report = SearchExecutionReport::from_summary_fields(&fields, Vec::new());
        let availability = report.solution_set_availability();
        assert!(!availability.contract_valid());
        assert!(!availability.uses_legacy_inference());
        assert!(!availability.solution_count_calculated());
        assert!(!availability.solution_set_materialized());
        assert!(!availability.solution_keys_complete());
        assert!(!availability.solution_page_available());
    }
}

#[test]
fn normal_explicit_contract_allows_distinct_raw_and_normalized_counts() {
    let fields = vec![
        ("search_output_policy".to_owned(), "summary".to_owned()),
        ("unique_solution_count".to_owned(), "3".to_owned()),
        (
            "normalized_unique_solution_count".to_owned(),
            "2".to_owned(),
        ),
        ("solution_count_calculated".to_owned(), "true".to_owned()),
        ("solution_set_materialized".to_owned(), "true".to_owned()),
        (
            "solution_keys_materialized_count".to_owned(),
            "2".to_owned(),
        ),
        ("solution_keys_complete".to_owned(), "true".to_owned()),
        ("solution_page_available".to_owned(), "false".to_owned()),
    ];

    let report = SearchExecutionReport::from_summary_fields(&fields, Vec::new());
    let availability = report.solution_set_availability();
    assert!(availability.contract_valid());
    assert!(availability.solution_count_calculated());
    assert!(availability.solution_set_materialized());
    assert!(availability.solution_keys_complete());
}

#[test]
fn coverage_summary_optional_mirror_hash_must_use_the_unavailable_sentinel() {
    let base = vec![
        (
            "search_output_policy".to_owned(),
            "coverage-summary".to_owned(),
        ),
        (
            "unique_solution_count".to_owned(),
            "not-calculated".to_owned(),
        ),
        (
            "normalized_unique_solution_count".to_owned(),
            "not-calculated".to_owned(),
        ),
        (
            "normalized_solution_set_hash".to_owned(),
            "not-calculated".to_owned(),
        ),
        (
            "actual_normalized_solution_set_hash".to_owned(),
            "not-calculated".to_owned(),
        ),
        ("solution_count_calculated".to_owned(), "false".to_owned()),
        ("solution_set_materialized".to_owned(), "false".to_owned()),
        (
            "solution_keys_materialized_count".to_owned(),
            "0".to_owned(),
        ),
        ("solution_keys_complete".to_owned(), "false".to_owned()),
        ("solution_page_available".to_owned(), "false".to_owned()),
    ];
    let mut valid = base.clone();
    valid.push((
        "mirror_normalized_solution_set_hash".to_owned(),
        "not-calculated".to_owned(),
    ));
    assert!(
        SearchExecutionReport::from_summary_fields(&valid, Vec::new())
            .solution_set_availability()
            .contract_valid()
    );

    let mut invalid = base;
    invalid.push((
        "mirror_normalized_solution_set_hash".to_owned(),
        "cts1:fake".to_owned(),
    ));
    assert!(
        !SearchExecutionReport::from_summary_fields(&invalid, Vec::new())
            .solution_set_availability()
            .contract_valid()
    );
}
