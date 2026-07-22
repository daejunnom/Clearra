#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceTruncationReason {
    FrontierBudgetExceeded,
    CandidateBudgetExceeded,
    HashBucketBudgetExceeded,
    GpuBatchBytesExceeded,
    ReadbackBytesExceeded,
    BuildWorkerBacklogExceeded,
    CoverageRowsExceeded,
    PatternBitsExceeded,
    CpuTimeExceeded,
    MemoryExceeded,
    ObservedUniverseTruncated,
    OperationTableCapacityExceeded,
    PruningEvidenceCapacityExceeded,
}

impl ResourceTruncationReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FrontierBudgetExceeded => "frontier_budget_exceeded",
            Self::CandidateBudgetExceeded => "candidate_budget_exceeded",
            Self::HashBucketBudgetExceeded => "hash_bucket_budget_exceeded",
            Self::GpuBatchBytesExceeded => "gpu_batch_bytes_exceeded",
            Self::ReadbackBytesExceeded => "readback_bytes_exceeded",
            Self::BuildWorkerBacklogExceeded => "build_worker_backlog_exceeded",
            Self::CoverageRowsExceeded => "coverage_rows_exceeded",
            Self::PatternBitsExceeded => "pattern_bits_exceeded",
            Self::CpuTimeExceeded => "cpu_time_exceeded",
            Self::MemoryExceeded => "memory_exceeded",
            Self::ObservedUniverseTruncated => "observed_universe_truncated",
            Self::OperationTableCapacityExceeded => "operation_table_capacity_exceeded",
            Self::PruningEvidenceCapacityExceeded => "pruning_evidence_capacity_exceeded",
        }
    }
}
