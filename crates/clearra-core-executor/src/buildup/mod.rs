pub mod buildup_candidate_acceptance;
pub mod buildup_coverage_bridge;
pub mod buildup_error;
pub mod buildup_execution_mode;
mod buildup_geometry_language_evaluator;
mod buildup_geometry_language_execution;
pub mod buildup_memo_key;
pub mod buildup_native_bridge;
pub mod buildup_objective_bridge;
mod buildup_parallelism;
pub mod buildup_replay_bridge;
pub mod buildup_run_result;
pub mod buildup_runner;
#[cfg(test)]
mod buildup_runner_tests;
mod buildup_solution_probability;
pub mod buildup_solution_set_contract;
pub mod buildup_trace_retention;
mod buildup_unique_solution_search;
#[cfg(test)]
mod candidate_execution_aggregate;
#[cfg(test)]
mod candidate_execution_aggregate_builder;
mod execution_variant_set;
pub mod generic_buildup;
mod objective_incomplete_reason;
mod objective_pattern_input_materializer;
mod objective_pattern_inputs;
mod objective_pattern_materialization;
mod objective_reduction_outcome;

pub(crate) use execution_variant_set::ExecutionVariantSet;

use clearra_core_ffi::{CBuildUpEvent, CBuildUpState, CResultReducerCounts};

pub use buildup_candidate_acceptance::BuildUpCandidateAcceptance;
pub use buildup_error::BuildUpRunnerError;
pub use buildup_execution_mode::BuildUpExecutionMode;
pub use buildup_memo_key::{BuildUpMemoKey, CacheIdentity, DeletedLineState};
pub use buildup_run_result::BuildUpRunResult;
pub use buildup_runner::BuildUpRunner;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BuildUpState {
    raw: CBuildUpState,
}

impl BuildUpState {
    pub fn from_raw(raw: CBuildUpState) -> Self {
        Self { raw }
    }
}
impl BuildUpState {
    pub fn raw(self) -> CBuildUpState {
        self.raw
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BuildUpEvent {
    raw: CBuildUpEvent,
}

impl BuildUpEvent {
    pub fn from_raw(raw: CBuildUpEvent) -> Self {
        Self { raw }
    }
}
impl BuildUpEvent {
    pub fn raw(self) -> CBuildUpEvent {
        self.raw
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BuildUpReducerReport {
    counts: CResultReducerCounts,
}

impl BuildUpReducerReport {
    pub fn new(counts: CResultReducerCounts) -> Self {
        Self { counts }
    }
}
impl BuildUpReducerReport {
    pub fn total_solution_count(self) -> u64 {
        self.counts.total_solution_count
    }
}
impl BuildUpReducerReport {
    pub fn retained_trace_count(self) -> u32 {
        self.counts.retained_trace_count
    }
}
impl BuildUpReducerReport {
    pub fn count_complete(self) -> bool {
        self.counts.count_complete != 0
    }
}
impl BuildUpReducerReport {
    pub fn trace_retention_truncated(self) -> bool {
        self.counts.trace_retention_truncated != 0
    }
}
