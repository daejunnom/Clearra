use clearra_setup_search::{
    coverage::{SETUP_RAW_COVERAGE_EXPORT_KIND, SETUP_RAW_COVERAGE_EXPORT_SCHEMA_VERSION},
    evaluate::{SETUP_RAW_METRICS_KIND, SETUP_RAW_METRICS_SCHEMA_VERSION},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupRawMetricsSchema {
    schema_version: u32,
    metrics_kind: &'static str,
    required_fields: Vec<&'static str>,
    forbidden_fields: Vec<&'static str>,
}

impl SetupRawMetricsSchema {
    pub fn v2() -> Self {
        Self {
            schema_version: SETUP_RAW_METRICS_SCHEMA_VERSION,
            metrics_kind: SETUP_RAW_METRICS_KIND,
            required_fields: vec![
                "shape_family_id",
                "shape_family_count",
                "tiling_variant_count",
                "build_variant_count",
                "covered_pattern_count",
                "coverage_probability",
                "post_pc_solution_count",
                "score_basis",
                "score_aggregation_attached",
                "backend_report",
                "raw_coverage_export_path",
                "setup_raw_metrics",
                "setup_raw_coverage_export",
                "coverage_overlap_report",
                "build_variant_metrics",
                "diagnostic_evidence",
            ],
            forbidden_fields: vec!["condition_summary", "setup_condition_summary"],
        }
    }
}
impl SetupRawMetricsSchema {
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }
}
impl SetupRawMetricsSchema {
    pub fn metrics_kind(&self) -> &'static str {
        self.metrics_kind
    }
}
impl SetupRawMetricsSchema {
    pub fn required_fields(&self) -> &[&'static str] {
        &self.required_fields
    }
}
impl SetupRawMetricsSchema {
    pub fn forbidden_fields(&self) -> &[&'static str] {
        &self.forbidden_fields
    }
}
impl SetupRawMetricsSchema {
    pub fn requires_field(&self, field: &str) -> bool {
        self.required_fields.iter().any(|known| known == &field)
    }
}
impl SetupRawMetricsSchema {
    pub fn forbids_field(&self, field: &str) -> bool {
        self.forbidden_fields.iter().any(|known| known == &field)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupRawCoverageExportSchema {
    schema_version: u32,
    export_kind: &'static str,
    required_fields: Vec<&'static str>,
}

impl SetupRawCoverageExportSchema {
    pub fn v2() -> Self {
        Self {
            schema_version: SETUP_RAW_COVERAGE_EXPORT_SCHEMA_VERSION,
            export_kind: SETUP_RAW_COVERAGE_EXPORT_KIND,
            required_fields: vec![
                "pattern_universe_id",
                "pattern_weight_model_id",
                "pattern_count",
                "rows",
                "family_unions",
                "overlap_report",
            ],
        }
    }
}
impl SetupRawCoverageExportSchema {
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }
}
impl SetupRawCoverageExportSchema {
    pub fn export_kind(&self) -> &'static str {
        self.export_kind
    }
}
impl SetupRawCoverageExportSchema {
    pub fn required_fields(&self) -> &[&'static str] {
        &self.required_fields
    }
}
impl SetupRawCoverageExportSchema {
    pub fn requires_field(&self, field: &str) -> bool {
        self.required_fields.iter().any(|known| known == &field)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gui_setup_explorer_consumes_raw_metrics_schema() {
        let raw_metrics = SetupRawMetricsSchema::v2();
        let raw_coverage = SetupRawCoverageExportSchema::v2();

        assert_eq!(raw_metrics.schema_version(), 2);
        assert_eq!(raw_metrics.metrics_kind(), "setup_raw_metrics");
        assert!(raw_metrics.requires_field("shape_family_id"));
        assert!(raw_metrics.requires_field("coverage_overlap_report"));
        assert!(raw_metrics.forbids_field("condition_summary"));
        assert_eq!(raw_coverage.schema_version(), 2);
        assert_eq!(raw_coverage.export_kind(), "setup_raw_coverage_export");
        assert!(raw_coverage.requires_field("rows"));
        assert!(raw_coverage.requires_field("family_unions"));
        assert!(raw_coverage.requires_field("overlap_report"));
    }
}
