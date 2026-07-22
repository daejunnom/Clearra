pub const SETUP_RAW_METRICS_SCHEMA_VERSION: u32 = 2;
pub const SETUP_RAW_METRICS_KIND: &str = "setup_raw_metrics";

#[derive(Clone, Debug, PartialEq)]
pub struct SetupRawMetricsV2 {
    schema_version: u32,
    metrics_kind: &'static str,
    shape_family_id: String,
    shape_family_count: usize,
    tiling_variant_count: usize,
    build_variant_count: usize,
    covered_pattern_count: usize,
    coverage_probability: f64,
    post_pc_solution_count: usize,
    score_basis: String,
    score_aggregation_attached: bool,
    backend_report: String,
    raw_coverage_export_path: String,
    setup_raw_metrics: String,
    setup_raw_coverage_export: String,
    coverage_overlap_report: String,
    build_variant_metrics: String,
    diagnostic_evidence: String,
}

impl SetupRawMetricsV2 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        shape_family_id: impl Into<String>,
        shape_family_count: usize,
        tiling_variant_count: usize,
        build_variant_count: usize,
        covered_pattern_count: usize,
        coverage_probability: f64,
        post_pc_solution_count: usize,
        score_basis: impl Into<String>,
        score_aggregation_attached: bool,
        backend_report: impl Into<String>,
        raw_coverage_export_path: impl Into<String>,
        coverage_overlap_report: impl Into<String>,
        build_variant_metrics: impl Into<String>,
        diagnostic_evidence: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: SETUP_RAW_METRICS_SCHEMA_VERSION,
            metrics_kind: SETUP_RAW_METRICS_KIND,
            shape_family_id: shape_family_id.into(),
            shape_family_count,
            tiling_variant_count,
            build_variant_count,
            covered_pattern_count,
            coverage_probability,
            post_pc_solution_count,
            score_basis: score_basis.into(),
            score_aggregation_attached,
            backend_report: backend_report.into(),
            raw_coverage_export_path: raw_coverage_export_path.into(),
            setup_raw_metrics: "attached".to_owned(),
            setup_raw_coverage_export: "inline".to_owned(),
            coverage_overlap_report: coverage_overlap_report.into(),
            build_variant_metrics: build_variant_metrics.into(),
            diagnostic_evidence: diagnostic_evidence.into(),
        }
    }
}
impl SetupRawMetricsV2 {
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }
}
impl SetupRawMetricsV2 {
    pub fn metrics_kind(&self) -> &'static str {
        self.metrics_kind
    }
}
impl SetupRawMetricsV2 {
    pub fn shape_family_id(&self) -> &str {
        &self.shape_family_id
    }
}
impl SetupRawMetricsV2 {
    pub fn shape_family_count(&self) -> usize {
        self.shape_family_count
    }
}
impl SetupRawMetricsV2 {
    pub fn tiling_variant_count(&self) -> usize {
        self.tiling_variant_count
    }
}
impl SetupRawMetricsV2 {
    pub fn build_variant_count(&self) -> usize {
        self.build_variant_count
    }
}
impl SetupRawMetricsV2 {
    pub fn covered_pattern_count(&self) -> usize {
        self.covered_pattern_count
    }
}
impl SetupRawMetricsV2 {
    pub fn coverage_probability(&self) -> f64 {
        self.coverage_probability
    }
}
impl SetupRawMetricsV2 {
    pub fn post_pc_solution_count(&self) -> usize {
        self.post_pc_solution_count
    }
}
impl SetupRawMetricsV2 {
    pub fn score_basis(&self) -> &str {
        &self.score_basis
    }
}
impl SetupRawMetricsV2 {
    pub fn score_aggregation_attached(&self) -> bool {
        self.score_aggregation_attached
    }
}
impl SetupRawMetricsV2 {
    pub fn backend_report(&self) -> &str {
        &self.backend_report
    }
}
impl SetupRawMetricsV2 {
    pub fn raw_coverage_export_path(&self) -> &str {
        &self.raw_coverage_export_path
    }
}
impl SetupRawMetricsV2 {
    pub fn setup_raw_metrics(&self) -> &str {
        &self.setup_raw_metrics
    }
}
impl SetupRawMetricsV2 {
    pub fn setup_raw_coverage_export(&self) -> &str {
        &self.setup_raw_coverage_export
    }
}
impl SetupRawMetricsV2 {
    pub fn coverage_overlap_report(&self) -> &str {
        &self.coverage_overlap_report
    }
}
impl SetupRawMetricsV2 {
    pub fn build_variant_metrics(&self) -> &str {
        &self.build_variant_metrics
    }
}
impl SetupRawMetricsV2 {
    pub fn diagnostic_evidence(&self) -> &str {
        &self.diagnostic_evidence
    }
}
impl SetupRawMetricsV2 {
    pub fn raw_metrics_sufficient_for_filtering(&self) -> bool {
        self.schema_version == SETUP_RAW_METRICS_SCHEMA_VERSION
            && self.metrics_kind == SETUP_RAW_METRICS_KIND
            && !self.shape_family_id.is_empty()
            && !self.backend_report.is_empty()
            && !self.raw_coverage_export_path.is_empty()
            && !self.coverage_overlap_report.is_empty()
            && !self.build_variant_metrics.is_empty()
            && !self.diagnostic_evidence.is_empty()
    }
}
impl SetupRawMetricsV2 {
    pub fn interpreted_summary_absent(&self) -> bool {
        true
    }
}

#[cfg(test)]
#[path = "setup_raw_metrics_v2_tests.rs"]
mod tests;
