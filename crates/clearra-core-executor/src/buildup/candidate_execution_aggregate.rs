use clearra_core_ffi::CBuildVariantView;
use clearra_coverage::row::coverage_row::CoverageRow;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CandidateExecutionAggregate {
    candidate_id: u64,
    coverage_row: CoverageRow,
    execution_variants: Vec<CBuildVariantView>,
    representative_trace: Option<String>,
}

impl CandidateExecutionAggregate {
    pub(crate) fn new(
        coverage_row: CoverageRow,
        execution_variants: Vec<CBuildVariantView>,
        representative_trace: Option<String>,
    ) -> Self {
        Self {
            candidate_id: coverage_row.candidate_id(),
            coverage_row,
            execution_variants,
            representative_trace,
        }
    }
}
impl CandidateExecutionAggregate {
    pub(crate) const fn candidate_id(&self) -> u64 {
        self.candidate_id
    }
}
impl CandidateExecutionAggregate {
    pub(crate) fn coverage_row(&self) -> &CoverageRow {
        &self.coverage_row
    }
}
impl CandidateExecutionAggregate {
    #[cfg(test)]
    pub(crate) fn execution_variants(&self) -> &[CBuildVariantView] {
        &self.execution_variants
    }
}
impl CandidateExecutionAggregate {
    pub(crate) fn stable_key(&self) -> &str {
        self.representative_trace
            .as_deref()
            .unwrap_or("candidate-without-retained-trace")
    }
}
