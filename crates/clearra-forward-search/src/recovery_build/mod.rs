//! Paired Build targets with canonical queue-pattern input. Target geometry is
//! generated lazily by lifting exact reachable locks into two logical Build
//! regions. This is the Build/Verify product, not a fixed-role approximation.
//! No isolated-stage B2B or reachability failure is used as a joint prune.
mod field;
mod parallel;
mod parallel_wire;
mod population;
pub use parallel::{
    RecoveryBuildCoordinator, RecoveryBuildParallelProgress, RecoveryBuildProduce,
    RecoveryBuildWorker, RECOVERY_PAIRS_PER_TASK,
};
mod search;
#[cfg(test)]
mod tests;
pub use field::RecoveryBuildFields;
pub use population::{RecoveryBuildExample, RecoveryBuildPopulation, RecoveryBuildQuery};
pub use search::{
    RecoveryBuildFixedQuery, RecoveryBuildFixedReport, RecoveryBuildStatus, RecoveryBuildStep,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryBuildError {
    InvalidHeight,
    BoardOutsideField,
    MiddleOverlapsStart,
    ResultOverlapsRetainedMiddle,
    TargetAreaNotTetrominoes,
    EmptySupply,
    InvalidSupplyPattern,
    PatternDomainUnavailable,
    CounterOverflow,
    MemoryUnavailable,
    UnsupportedRuleProfile,
    Cancelled,
    InvalidParallelWire,
    InvalidParallelState,
}

#[cfg(test)]
mod parallel_tests;
