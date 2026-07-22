use super::ResourceTruncationReason;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceReport {
    pub truncated: bool,
    pub truncation_reason: Option<ResourceTruncationReason>,
    pub peak_frontier_states: usize,
    pub peak_candidate_rows: usize,
    pub peak_hash_buckets: usize,
    pub peak_gpu_bytes: usize,
    pub peak_cpu_bytes: usize,
    pub build_worker_backlog_peak: usize,
    pub coverage_rows_emitted: usize,
    pub probability_complete: bool,
}

impl ResourceReport {
    pub const fn complete() -> Self {
        Self {
            truncated: false,
            truncation_reason: None,
            peak_frontier_states: 0,
            peak_candidate_rows: 0,
            peak_hash_buckets: 0,
            peak_gpu_bytes: 0,
            peak_cpu_bytes: 0,
            build_worker_backlog_peak: 0,
            coverage_rows_emitted: 0,
            probability_complete: true,
        }
    }
}
impl ResourceReport {
    pub fn mark_truncated(&mut self, reason: ResourceTruncationReason) {
        self.truncated = true;
        self.truncation_reason.get_or_insert(reason);
        self.probability_complete = false;
    }
}
impl ResourceReport {
    pub fn observe_frontier_states(&mut self, value: usize) {
        self.peak_frontier_states = self.peak_frontier_states.max(value);
    }
}
impl ResourceReport {
    pub fn observe_candidate_rows(&mut self, value: usize) {
        self.peak_candidate_rows = self.peak_candidate_rows.max(value);
    }
}
impl ResourceReport {
    pub fn observe_hash_buckets(&mut self, value: usize) {
        self.peak_hash_buckets = self.peak_hash_buckets.max(value);
    }
}
impl ResourceReport {
    pub fn observe_gpu_bytes(&mut self, value: usize) {
        self.peak_gpu_bytes = self.peak_gpu_bytes.max(value);
    }
}
impl ResourceReport {
    pub fn observe_cpu_bytes(&mut self, value: usize) {
        self.peak_cpu_bytes = self.peak_cpu_bytes.max(value);
    }
}
impl ResourceReport {
    pub fn observe_build_worker_backlog(&mut self, value: usize) {
        self.build_worker_backlog_peak = self.build_worker_backlog_peak.max(value);
    }
}
impl ResourceReport {
    pub fn observe_coverage_rows(&mut self, value: usize) {
        self.coverage_rows_emitted = self.coverage_rows_emitted.max(value);
    }
}

impl Default for ResourceReport {
    fn default() -> Self {
        Self::complete()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_budget_exceeded_probability_complete_false() {
        let mut report = ResourceReport::complete();

        report.mark_truncated(ResourceTruncationReason::CoverageRowsExceeded);

        assert!(report.truncated);
        assert_eq!(
            report.truncation_reason,
            Some(ResourceTruncationReason::CoverageRowsExceeded)
        );
        assert!(!report.probability_complete);
    }
}
