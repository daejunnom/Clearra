use clearra_core_domain::resource::{ResourceBudget, ResourceReport, ResourceTruncationReason};

mod dense_pattern_preflight;
mod durable_delegation_journal;
mod execution_admission;
mod shared_execution_resource_authority;

pub(crate) use dense_pattern_preflight::{
    preflight_dense_pattern_execution, DensePatternExecutionSurface, DensePatternPreflight,
};
pub use durable_delegation_journal::{
    default_native_delegation_journal_root, DelegationBudget, DelegationEvent,
    DelegationEventDraft, DelegationIdentity, DelegationJournal, DelegationJournalError,
    DelegationPhase, DelegationToken, DurableDelegationAuthority, DurableDelegationError,
    ExecutableDelegationPermit, MemoryDelegationJournal, NativeJsonlDelegationJournal,
    ResultApplicationDecision, ACTIVE_LEASE_EXPIRY_MS, OFFER_ACCEPT_TIMEOUT_MS,
    PERSISTED_RENEWAL_INTERVAL_MS, TERMINAL_TOMBSTONE_RETENTION_MS, WORKER_HEARTBEAT_INTERVAL_MS,
};
pub(crate) use execution_admission::{
    admit_budget_bound_search_execution,
    admit_budget_bound_search_execution_under_terminal_authority, admit_search_execution,
    ExecutionAdmission, ExecutionAdmissionPlan, ExecutionMemoryBound,
};
pub use shared_execution_resource_authority::WasmCpuTerminalResourceAuthority;
pub(crate) use shared_execution_resource_authority::{
    acquire_shared_execution_resources, next_execution_resource_owner,
    shared_execution_resource_capacity,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchResourceTracker {
    budget: ResourceBudget,
    report: ResourceReport,
}

impl BatchResourceTracker {
    pub fn new(budget: ResourceBudget) -> Self {
        Self {
            budget,
            report: ResourceReport::complete(),
        }
    }
}
impl BatchResourceTracker {
    pub fn observe_frontier_states(&mut self, value: usize) -> bool {
        self.report.observe_frontier_states(value);
        self.guard(
            value <= self.budget.max_frontier_states,
            ResourceTruncationReason::FrontierBudgetExceeded,
        )
    }
}
impl BatchResourceTracker {
    pub fn observe_candidate_rows(&mut self, value: usize) -> bool {
        self.report.observe_candidate_rows(value);
        self.guard(
            value <= self.budget.max_candidate_rows,
            ResourceTruncationReason::CandidateBudgetExceeded,
        )
    }
}
impl BatchResourceTracker {
    pub fn observe_hash_buckets(&mut self, value: usize) -> bool {
        self.report.observe_hash_buckets(value);
        self.guard(
            value <= self.budget.max_hash_buckets,
            ResourceTruncationReason::HashBucketBudgetExceeded,
        )
    }
}
impl BatchResourceTracker {
    pub fn observe_gpu_bytes(&mut self, value: usize) -> bool {
        self.report.observe_gpu_bytes(value);
        self.guard(
            value <= self.budget.max_gpu_batch_bytes,
            ResourceTruncationReason::GpuBatchBytesExceeded,
        )
    }
}
impl BatchResourceTracker {
    pub fn observe_readback_bytes(&mut self, value: usize) -> bool {
        self.guard(
            value <= self.budget.max_readback_bytes,
            ResourceTruncationReason::ReadbackBytesExceeded,
        )
    }
}
impl BatchResourceTracker {
    pub fn observe_build_worker_backlog(&mut self, value: usize) -> bool {
        self.report.observe_build_worker_backlog(value);
        self.guard(
            value <= self.budget.max_build_worker_backlog,
            ResourceTruncationReason::BuildWorkerBacklogExceeded,
        )
    }
}
impl BatchResourceTracker {
    pub fn observe_coverage_rows(&mut self, value: usize) -> bool {
        self.report.observe_coverage_rows(value);
        self.guard(
            value <= self.budget.max_coverage_rows,
            ResourceTruncationReason::CoverageRowsExceeded,
        )
    }
}
impl BatchResourceTracker {
    pub fn observe_pattern_bits(&mut self, value: usize) -> bool {
        self.guard(
            value <= self.budget.max_pattern_bits,
            ResourceTruncationReason::PatternBitsExceeded,
        )
    }
}
impl BatchResourceTracker {
    pub fn mark_observed_universe_truncated(&mut self) {
        self.report
            .mark_truncated(ResourceTruncationReason::ObservedUniverseTruncated);
    }
}
impl BatchResourceTracker {
    pub fn report(&self) -> &ResourceReport {
        &self.report
    }
}
impl BatchResourceTracker {
    pub fn finish(self) -> ResourceReport {
        self.report
    }
}
impl BatchResourceTracker {
    fn guard(&mut self, ok: bool, reason: ResourceTruncationReason) -> bool {
        if !ok {
            self.report.mark_truncated(reason);
        }
        ok
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
