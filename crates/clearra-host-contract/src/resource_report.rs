#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ResourceReport {
    solver_executed: bool,
    memory_status: String,
    pub truncated: bool,
    pub truncation_reason: Option<String>,
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
    pub fn new(solver_executed: bool, memory_status: impl Into<String>) -> Self {
        Self {
            solver_executed,
            memory_status: memory_status.into(),
            truncated: false,
            truncation_reason: None,
            peak_frontier_states: 0,
            peak_candidate_rows: 0,
            peak_hash_buckets: 0,
            peak_gpu_bytes: 0,
            peak_cpu_bytes: 0,
            build_worker_backlog_peak: 0,
            coverage_rows_emitted: 0,
            probability_complete: solver_executed,
        }
    }
}
impl ResourceReport {
    pub fn with_truncation(mut self, reason: impl Into<String>) -> Self {
        self.truncated = true;
        self.truncation_reason = Some(reason.into());
        self.probability_complete = false;
        self
    }
}
impl ResourceReport {
    pub const fn solver_executed(&self) -> bool {
        self.solver_executed
    }
}
impl ResourceReport {
    pub fn memory_status(&self) -> &str {
        &self.memory_status
    }
}
impl ResourceReport {
    pub const fn truncated(&self) -> bool {
        self.truncated
    }
}
impl ResourceReport {
    pub fn truncation_reason(&self) -> Option<&str> {
        self.truncation_reason.as_deref()
    }
}
impl ResourceReport {
    pub const fn probability_complete(&self) -> bool {
        self.probability_complete
    }
}

impl Default for ResourceReport {
    fn default() -> Self {
        Self::new(false, "not-executed")
    }
}
