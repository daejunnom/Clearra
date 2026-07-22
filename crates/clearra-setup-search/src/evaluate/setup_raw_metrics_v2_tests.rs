use super::*;

fn example_metrics() -> SetupRawMetricsV2 {
    SetupRawMetricsV2::new(
        "setup-family-0",
        1,
        2,
        3,
        5,
        0.625,
        13,
        "retained-traces",
        true,
        "attached",
        "inline://clearra/setup/raw-coverage/setup-family-0/union",
        "overlap-visible",
        "per-build-variant",
        "attached",
    )
}

#[test]
fn raw_metrics_sufficient_for_filtering() {
    let metrics = example_metrics();

    assert_eq!(metrics.schema_version(), 2);
    assert_eq!(metrics.metrics_kind(), "setup_raw_metrics");
    assert_eq!(metrics.shape_family_id(), "setup-family-0");
    assert_eq!(metrics.shape_family_count(), 1);
    assert_eq!(metrics.tiling_variant_count(), 2);
    assert_eq!(metrics.build_variant_count(), 3);
    assert_eq!(metrics.covered_pattern_count(), 5);
    assert_eq!(metrics.coverage_probability(), 0.625);
    assert_eq!(metrics.post_pc_solution_count(), 13);
    assert_eq!(metrics.score_basis(), "retained-traces");
    assert!(metrics.score_aggregation_attached());
    assert_eq!(metrics.backend_report(), "attached");
    assert_eq!(metrics.setup_raw_metrics(), "attached");
    assert_eq!(metrics.setup_raw_coverage_export(), "inline");
    assert_eq!(metrics.coverage_overlap_report(), "overlap-visible");
    assert_eq!(metrics.build_variant_metrics(), "per-build-variant");
    assert_eq!(metrics.diagnostic_evidence(), "attached");
    assert!(metrics.raw_metrics_sufficient_for_filtering());
}

#[test]
fn condition_summary_field_absent() {
    let metrics = example_metrics();

    assert!(metrics.interpreted_summary_absent());
}
