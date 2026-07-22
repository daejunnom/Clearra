pub const SETUP_RAW_COVERAGE_EXPORT_SCHEMA_VERSION: u32 = 2;
pub const SETUP_RAW_COVERAGE_EXPORT_KIND: &str = "setup_raw_coverage_export";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupRawCoverageRow {
    row_id: u64,
    family_id: String,
    candidate_id: u64,
    covered_pattern_ids: Vec<usize>,
}

impl SetupRawCoverageRow {
    pub fn new(
        row_id: u64,
        family_id: impl Into<String>,
        candidate_id: u64,
        covered_pattern_ids: Vec<usize>,
    ) -> Self {
        Self {
            row_id,
            family_id: family_id.into(),
            candidate_id,
            covered_pattern_ids,
        }
    }
}
impl SetupRawCoverageRow {
    pub fn row_id(&self) -> u64 {
        self.row_id
    }
}
impl SetupRawCoverageRow {
    pub fn family_id(&self) -> &str {
        &self.family_id
    }
}
impl SetupRawCoverageRow {
    pub fn candidate_id(&self) -> u64 {
        self.candidate_id
    }
}
impl SetupRawCoverageRow {
    pub fn covered_pattern_ids(&self) -> &[usize] {
        &self.covered_pattern_ids
    }
}
impl SetupRawCoverageRow {
    pub fn covered_pattern_count(&self) -> usize {
        self.covered_pattern_ids.len()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SetupRawCoverageFamilyUnion {
    family_id: String,
    covered_pattern_ids: Vec<usize>,
    coverage_probability: f64,
}

impl SetupRawCoverageFamilyUnion {
    pub fn new(
        family_id: impl Into<String>,
        covered_pattern_ids: Vec<usize>,
        coverage_probability: f64,
    ) -> Self {
        Self {
            family_id: family_id.into(),
            covered_pattern_ids,
            coverage_probability,
        }
    }
}
impl SetupRawCoverageFamilyUnion {
    pub fn family_id(&self) -> &str {
        &self.family_id
    }
}
impl SetupRawCoverageFamilyUnion {
    pub fn covered_pattern_ids(&self) -> &[usize] {
        &self.covered_pattern_ids
    }
}
impl SetupRawCoverageFamilyUnion {
    pub fn covered_pattern_count(&self) -> usize {
        self.covered_pattern_ids.len()
    }
}
impl SetupRawCoverageFamilyUnion {
    pub fn coverage_probability(&self) -> f64 {
        self.coverage_probability
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupCoverageOverlapReport {
    visible: bool,
    overlapping_pattern_ids: Vec<usize>,
    duplicate_pattern_count: usize,
}

impl SetupCoverageOverlapReport {
    pub fn new(overlapping_pattern_ids: Vec<usize>, duplicate_pattern_count: usize) -> Self {
        Self {
            visible: true,
            overlapping_pattern_ids,
            duplicate_pattern_count,
        }
    }
}
impl SetupCoverageOverlapReport {
    pub fn none() -> Self {
        Self::new(Vec::new(), 0)
    }
}
impl SetupCoverageOverlapReport {
    pub fn is_visible(&self) -> bool {
        self.visible
    }
}
impl SetupCoverageOverlapReport {
    pub fn overlapping_pattern_ids(&self) -> &[usize] {
        &self.overlapping_pattern_ids
    }
}
impl SetupCoverageOverlapReport {
    pub fn duplicate_pattern_count(&self) -> usize {
        self.duplicate_pattern_count
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SetupRawCoverageExport {
    schema_version: u32,
    export_kind: &'static str,
    pattern_universe_id: u64,
    pattern_weight_model_id: u64,
    pattern_count: usize,
    rows: Vec<SetupRawCoverageRow>,
    family_unions: Vec<SetupRawCoverageFamilyUnion>,
    overlap_report: SetupCoverageOverlapReport,
}

impl SetupRawCoverageExport {
    pub fn new(
        pattern_universe_id: u64,
        pattern_weight_model_id: u64,
        pattern_count: usize,
        rows: Vec<SetupRawCoverageRow>,
        family_unions: Vec<SetupRawCoverageFamilyUnion>,
        overlap_report: SetupCoverageOverlapReport,
    ) -> Self {
        Self {
            schema_version: SETUP_RAW_COVERAGE_EXPORT_SCHEMA_VERSION,
            export_kind: SETUP_RAW_COVERAGE_EXPORT_KIND,
            pattern_universe_id,
            pattern_weight_model_id,
            pattern_count,
            rows,
            family_unions,
            overlap_report,
        }
    }
}
impl SetupRawCoverageExport {
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }
}
impl SetupRawCoverageExport {
    pub fn export_kind(&self) -> &'static str {
        self.export_kind
    }
}
impl SetupRawCoverageExport {
    pub fn pattern_universe_id(&self) -> u64 {
        self.pattern_universe_id
    }
}
impl SetupRawCoverageExport {
    pub fn pattern_weight_model_id(&self) -> u64 {
        self.pattern_weight_model_id
    }
}
impl SetupRawCoverageExport {
    pub fn pattern_count(&self) -> usize {
        self.pattern_count
    }
}
impl SetupRawCoverageExport {
    pub fn rows(&self) -> &[SetupRawCoverageRow] {
        &self.rows
    }
}
impl SetupRawCoverageExport {
    pub fn family_unions(&self) -> &[SetupRawCoverageFamilyUnion] {
        &self.family_unions
    }
}
impl SetupRawCoverageExport {
    pub fn overlap_report(&self) -> &SetupCoverageOverlapReport {
        &self.overlap_report
    }
}
impl SetupRawCoverageExport {
    pub fn to_machine_readable_snapshot(&self) -> SetupRawCoverageExportSnapshot {
        SetupRawCoverageExportSnapshot {
            schema_version: self.schema_version,
            export_kind: self.export_kind.to_owned(),
            pattern_universe_id: self.pattern_universe_id,
            pattern_weight_model_id: self.pattern_weight_model_id,
            pattern_count: self.pattern_count,
            rows: self.rows.clone(),
            family_unions: self.family_unions.clone(),
            overlap_report: self.overlap_report.clone(),
        }
    }
}
impl SetupRawCoverageExport {
    pub fn from_machine_readable_snapshot(snapshot: SetupRawCoverageExportSnapshot) -> Self {
        Self {
            schema_version: snapshot.schema_version,
            export_kind: SETUP_RAW_COVERAGE_EXPORT_KIND,
            pattern_universe_id: snapshot.pattern_universe_id,
            pattern_weight_model_id: snapshot.pattern_weight_model_id,
            pattern_count: snapshot.pattern_count,
            rows: snapshot.rows,
            family_unions: snapshot.family_unions,
            overlap_report: snapshot.overlap_report,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SetupRawCoverageExportSnapshot {
    pub schema_version: u32,
    pub export_kind: String,
    pub pattern_universe_id: u64,
    pub pattern_weight_model_id: u64,
    pub pattern_count: usize,
    pub rows: Vec<SetupRawCoverageRow>,
    pub family_unions: Vec<SetupRawCoverageFamilyUnion>,
    pub overlap_report: SetupCoverageOverlapReport,
}

#[cfg(test)]
#[path = "setup_raw_coverage_export_tests.rs"]
mod tests;
