use super::setup_result_column_schema::{
    column, SetupResultColumnSchema, SetupResultColumnSource, SetupResultColumnType,
};

pub(crate) fn setup_diagnostic_columns() -> Vec<SetupResultColumnSchema> {
    vec![
        column(
            "setup_raw_metrics",
            "Raw metrics",
            SetupResultColumnType::Text,
            SetupResultColumnSource::SetupRawMetrics,
        ),
        column(
            "setup_raw_metrics_schema_version",
            "Raw metrics schema",
            SetupResultColumnType::Integer,
            SetupResultColumnSource::SetupRawMetrics,
        ),
        column(
            "metrics_kind",
            "Metrics kind",
            SetupResultColumnType::Text,
            SetupResultColumnSource::SetupRawMetrics,
        ),
        column(
            "setup_raw_coverage_export",
            "Raw coverage",
            SetupResultColumnType::Text,
            SetupResultColumnSource::SetupRawMetrics,
        ),
        column(
            "raw_coverage_schema_version",
            "Raw coverage schema",
            SetupResultColumnType::Integer,
            SetupResultColumnSource::SetupRawMetrics,
        ),
        column(
            "raw_coverage_export_kind",
            "Raw coverage kind",
            SetupResultColumnType::Text,
            SetupResultColumnSource::SetupRawMetrics,
        ),
        column(
            "raw_coverage_export_path",
            "Coverage export",
            SetupResultColumnType::Text,
            SetupResultColumnSource::SetupRawMetrics,
        ),
        column(
            "pattern_universe_id",
            "Pattern universe",
            SetupResultColumnType::Text,
            SetupResultColumnSource::SetupRawMetrics,
        ),
        column(
            "pattern_weight_model_id",
            "Pattern weights",
            SetupResultColumnType::Text,
            SetupResultColumnSource::SetupRawMetrics,
        ),
        column(
            "pattern_count",
            "Pattern count",
            SetupResultColumnType::Integer,
            SetupResultColumnSource::SetupRawMetrics,
        ),
        column(
            "rows",
            "Coverage rows",
            SetupResultColumnType::Text,
            SetupResultColumnSource::SetupRawMetrics,
        ),
        column(
            "family_unions",
            "Family unions",
            SetupResultColumnType::Text,
            SetupResultColumnSource::SetupRawMetrics,
        ),
        column(
            "overlap_report",
            "Overlap report",
            SetupResultColumnType::Text,
            SetupResultColumnSource::SetupRawMetrics,
        ),
        column(
            "coverage_overlap_report",
            "Coverage overlap",
            SetupResultColumnType::Text,
            SetupResultColumnSource::SetupRawMetrics,
        ),
        column(
            "backend_report",
            "Backend report",
            SetupResultColumnType::Text,
            SetupResultColumnSource::SetupRawMetrics,
        ),
        column(
            "search_unsupported_reason",
            "Unsupported reason",
            SetupResultColumnType::Text,
            SetupResultColumnSource::DiagnosticEvidence,
        ),
        column(
            "build_variant_metrics_required_hold",
            "Hold",
            SetupResultColumnType::Text,
            SetupResultColumnSource::BuildVariantMetrics,
        ),
        column(
            "diagnostic_evidence_rule_profile",
            "Rule",
            SetupResultColumnType::Text,
            SetupResultColumnSource::DiagnosticEvidence,
        ),
    ]
}
