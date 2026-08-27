use crate::{ExecutionAvailabilityReport, ExecutionCompletenessState, ExecutionSurface};

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
    execution_availability: ExecutionAvailabilityReport,
    result_completeness: ExecutionCompletenessState,
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
            probability_complete: false,
            execution_availability: if solver_executed {
                ExecutionAvailabilityReport::available(ExecutionSurface::current())
            } else {
                ExecutionAvailabilityReport::not_executed(ExecutionSurface::current())
            },
            result_completeness: if solver_executed {
                ExecutionCompletenessState::Incomplete
            } else {
                ExecutionCompletenessState::NotExecuted
            },
        }
    }

    /// Allocation-free owned-parts seam for a boundary that has already
    /// authorized all retained strings and nested availability evidence.
    #[allow(clippy::too_many_arguments)]
    pub fn from_owned_memory_authorized_parts(
        solver_executed: bool,
        memory_status: String,
        truncated: bool,
        truncation_reason: Option<String>,
        peak_frontier_states: usize,
        peak_candidate_rows: usize,
        peak_hash_buckets: usize,
        peak_gpu_bytes: usize,
        peak_cpu_bytes: usize,
        build_worker_backlog_peak: usize,
        coverage_rows_emitted: usize,
        probability_complete: bool,
        execution_availability: ExecutionAvailabilityReport,
        result_completeness: ExecutionCompletenessState,
    ) -> Self {
        Self {
            solver_executed,
            memory_status,
            truncated,
            truncation_reason,
            peak_frontier_states,
            peak_candidate_rows,
            peak_hash_buckets,
            peak_gpu_bytes,
            peak_cpu_bytes,
            build_worker_backlog_peak,
            coverage_rows_emitted,
            probability_complete,
            execution_availability,
            result_completeness,
        }
    }
}
impl ResourceReport {
    pub fn with_truncation(mut self, reason: impl Into<String>) -> Self {
        self.truncated = true;
        self.truncation_reason = Some(reason.into());
        self.probability_complete = false;
        self.result_completeness = ExecutionCompletenessState::Incomplete;
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
impl ResourceReport {
    pub fn with_execution_availability(
        mut self,
        execution_availability: ExecutionAvailabilityReport,
    ) -> Self {
        self.execution_availability = execution_availability;
        self
    }

    pub const fn execution_availability(&self) -> &ExecutionAvailabilityReport {
        &self.execution_availability
    }

    pub const fn result_completeness(&self) -> ExecutionCompletenessState {
        self.result_completeness
    }

    pub fn set_result_completeness(&mut self, completeness: ExecutionCompletenessState) {
        self.result_completeness = completeness;
    }

    /// Returns only heap payload transitively retained by this report.
    /// String values and nested availability evidence use their actual
    /// allocation capacities; inline counters and flags are excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = self.memory_status.capacity() as u128;
        if let Some(reason) = &self.truncation_reason {
            bytes = bytes.checked_add(reason.capacity() as u128)?;
        }
        bytes.checked_add(
            self.execution_availability
                .checked_retained_capacity_bytes()?,
        )
    }
}

impl Default for ResourceReport {
    fn default() -> Self {
        Self::new(false, "not-executed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExecutionAvailabilityState, ExecutionCompletenessState};

    #[test]
    fn default_is_neither_available_nor_complete() {
        let report = ResourceReport::default();
        assert!(!report.solver_executed());
        assert_eq!(
            report.execution_availability().state(),
            ExecutionAvailabilityState::Unavailable
        );
        assert_eq!(
            report.result_completeness(),
            ExecutionCompletenessState::NotExecuted
        );
    }

    #[test]
    fn available_and_complete_are_independent_axes() {
        let mut report = ResourceReport::new(true, "reported");
        assert_eq!(
            report.execution_availability().state(),
            ExecutionAvailabilityState::Available
        );
        assert_eq!(
            report.result_completeness(),
            ExecutionCompletenessState::Incomplete
        );
        assert!(!report.probability_complete());
        report.set_result_completeness(ExecutionCompletenessState::Complete);
        assert_eq!(
            report.result_completeness(),
            ExecutionCompletenessState::Complete
        );
    }

    #[test]
    fn retained_capacity_counts_status_reason_and_availability_evidence() {
        let mut memory_status = String::with_capacity(80);
        memory_status.push_str("reported");
        let memory_status_capacity = memory_status.capacity() as u128;
        let mut reason = String::with_capacity(112);
        reason.push_str("memory_exceeded");
        let reason_capacity = reason.capacity() as u128;
        let availability = ExecutionAvailabilityReport::exhausted(
            ExecutionSurface::BrowserWasm32,
            crate::ExecutionAvailabilityReason::MemoryBudgetExceeded,
        )
        .with_pattern_evidence(100, 200, 300)
        .with_required_memory_bytes(400);
        let availability_capacity = availability
            .checked_retained_capacity_bytes()
            .expect("availability capacity fits u128");
        let report = ResourceReport::new(true, memory_status)
            .with_truncation(reason)
            .with_execution_availability(availability);

        assert_eq!(
            report.checked_retained_capacity_bytes(),
            memory_status_capacity
                .checked_add(reason_capacity)
                .and_then(|bytes| bytes.checked_add(availability_capacity))
        );
    }
}
