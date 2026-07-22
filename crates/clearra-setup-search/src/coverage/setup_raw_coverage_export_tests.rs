use super::*;

fn example_export() -> SetupRawCoverageExport {
    SetupRawCoverageExport::new(
        1001,
        2001,
        4,
        vec![
            SetupRawCoverageRow::new(10, "setup-family-0", 10, vec![0, 1]),
            SetupRawCoverageRow::new(11, "setup-family-0", 11, vec![1, 2]),
        ],
        vec![SetupRawCoverageFamilyUnion::new(
            "setup-family-0",
            vec![0, 1, 2],
            0.75,
        )],
        SetupCoverageOverlapReport::new(vec![1], 1),
    )
}

#[test]
fn raw_coverage_export_roundtrip() {
    let export = example_export();
    let snapshot = export.to_machine_readable_snapshot();
    let restored = SetupRawCoverageExport::from_machine_readable_snapshot(snapshot);

    assert_eq!(restored, export);
    assert_eq!(restored.schema_version(), 2);
    assert_eq!(restored.export_kind(), "setup_raw_coverage_export");
    assert_eq!(restored.pattern_universe_id(), 1001);
    assert_eq!(restored.pattern_weight_model_id(), 2001);
    assert_eq!(restored.pattern_count(), 4);
    assert_eq!(restored.rows().len(), 2);
    assert_eq!(restored.family_unions()[0].covered_pattern_count(), 3);
}

#[test]
fn coverage_overlap_report_is_not_hidden() {
    let export = example_export();
    let overlap_report = export.overlap_report();

    assert!(overlap_report.is_visible());
    assert_eq!(overlap_report.overlapping_pattern_ids(), &[1]);
    assert_eq!(overlap_report.duplicate_pattern_count(), 1);
}
