use clearra_core_domain::resource::{ResourceReport, ResourceTruncationReason};

pub const C_RESOURCE_TRUNCATION_NONE: u16 = 0;
pub const C_RESOURCE_TRUNCATION_FRONTIER_BUDGET_EXCEEDED: u16 = 1;
pub const C_RESOURCE_TRUNCATION_CANDIDATE_BUDGET_EXCEEDED: u16 = 2;
pub const C_RESOURCE_TRUNCATION_HASH_BUCKET_BUDGET_EXCEEDED: u16 = 3;
pub const C_RESOURCE_TRUNCATION_GPU_BATCH_BYTES_EXCEEDED: u16 = 4;
pub const C_RESOURCE_TRUNCATION_READBACK_BYTES_EXCEEDED: u16 = 5;
pub const C_RESOURCE_TRUNCATION_BUILD_WORKER_BACKLOG_EXCEEDED: u16 = 6;
pub const C_RESOURCE_TRUNCATION_COVERAGE_ROWS_EXCEEDED: u16 = 7;
pub const C_RESOURCE_TRUNCATION_PATTERN_BITS_EXCEEDED: u16 = 8;
pub const C_RESOURCE_TRUNCATION_CPU_TIME_EXCEEDED: u16 = 9;
pub const C_RESOURCE_TRUNCATION_MEMORY_EXCEEDED: u16 = 10;
pub const C_RESOURCE_TRUNCATION_OBSERVED_UNIVERSE_TRUNCATED: u16 = 11;
pub const C_RESOURCE_TRUNCATION_OPERATION_TABLE_CAPACITY_EXCEEDED: u16 = 13;
pub const C_RESOURCE_TRUNCATION_PRUNING_EVIDENCE_CAPACITY_EXCEEDED: u16 = 14;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CNativeResourceReport {
    pub truncated: u8,
    pub probability_complete: u8,
    pub truncation_reason: u16,
    pub peak_frontier_states: usize,
    pub peak_candidate_rows: usize,
    pub peak_hash_buckets: usize,
    pub peak_gpu_bytes: usize,
    pub peak_cpu_bytes: usize,
    pub build_worker_backlog_peak: usize,
    pub coverage_rows_emitted: usize,
}

impl CNativeResourceReport {
    pub fn to_domain(self) -> ResourceReport {
        let mut report = ResourceReport::complete();
        report.peak_frontier_states = self.peak_frontier_states;
        report.peak_candidate_rows = self.peak_candidate_rows;
        report.peak_hash_buckets = self.peak_hash_buckets;
        report.peak_gpu_bytes = self.peak_gpu_bytes;
        report.peak_cpu_bytes = self.peak_cpu_bytes;
        report.build_worker_backlog_peak = self.build_worker_backlog_peak;
        report.coverage_rows_emitted = self.coverage_rows_emitted;
        report.probability_complete = self.probability_complete != 0 && self.truncated == 0;
        if self.truncated != 0 {
            if let Some(reason) = truncation_reason(self.truncation_reason) {
                report.mark_truncated(reason);
            } else {
                report.mark_truncated_unknown();
            }
        }
        report
    }
}

fn truncation_reason(reason: u16) -> Option<ResourceTruncationReason> {
    match reason {
        C_RESOURCE_TRUNCATION_NONE => None,
        C_RESOURCE_TRUNCATION_FRONTIER_BUDGET_EXCEEDED => {
            Some(ResourceTruncationReason::FrontierBudgetExceeded)
        }
        C_RESOURCE_TRUNCATION_CANDIDATE_BUDGET_EXCEEDED => {
            Some(ResourceTruncationReason::CandidateBudgetExceeded)
        }
        C_RESOURCE_TRUNCATION_HASH_BUCKET_BUDGET_EXCEEDED => {
            Some(ResourceTruncationReason::HashBucketBudgetExceeded)
        }
        C_RESOURCE_TRUNCATION_GPU_BATCH_BYTES_EXCEEDED => {
            Some(ResourceTruncationReason::GpuBatchBytesExceeded)
        }
        C_RESOURCE_TRUNCATION_READBACK_BYTES_EXCEEDED => {
            Some(ResourceTruncationReason::ReadbackBytesExceeded)
        }
        C_RESOURCE_TRUNCATION_BUILD_WORKER_BACKLOG_EXCEEDED => {
            Some(ResourceTruncationReason::BuildWorkerBacklogExceeded)
        }
        C_RESOURCE_TRUNCATION_COVERAGE_ROWS_EXCEEDED => {
            Some(ResourceTruncationReason::CoverageRowsExceeded)
        }
        C_RESOURCE_TRUNCATION_PATTERN_BITS_EXCEEDED => {
            Some(ResourceTruncationReason::PatternBitsExceeded)
        }
        C_RESOURCE_TRUNCATION_CPU_TIME_EXCEEDED => Some(ResourceTruncationReason::CpuTimeExceeded),
        C_RESOURCE_TRUNCATION_MEMORY_EXCEEDED => Some(ResourceTruncationReason::MemoryExceeded),
        C_RESOURCE_TRUNCATION_OBSERVED_UNIVERSE_TRUNCATED => {
            Some(ResourceTruncationReason::ObservedUniverseTruncated)
        }
        C_RESOURCE_TRUNCATION_OPERATION_TABLE_CAPACITY_EXCEEDED => {
            Some(ResourceTruncationReason::OperationTableCapacityExceeded)
        }
        C_RESOURCE_TRUNCATION_PRUNING_EVIDENCE_CAPACITY_EXCEEDED => {
            Some(ResourceTruncationReason::PruningEvidenceCapacityExceeded)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_candidate_budget_report_maps_to_incomplete_domain_report() {
        let report = CNativeResourceReport {
            truncated: 1,
            probability_complete: 0,
            truncation_reason: C_RESOURCE_TRUNCATION_CANDIDATE_BUDGET_EXCEEDED,
            peak_candidate_rows: 257,
            ..Default::default()
        }
        .to_domain();

        assert!(report.truncated);
        assert_eq!(
            report.truncation_reason,
            Some(ResourceTruncationReason::CandidateBudgetExceeded)
        );
        assert_eq!(report.peak_candidate_rows, 257);
        assert!(!report.probability_complete);
    }
}
