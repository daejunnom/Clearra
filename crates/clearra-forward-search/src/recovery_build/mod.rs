//! Stage-factorized paired Build coverage with demand-driven boundary repair.
//! The original weighted supply universe is preserved without pair expansion.
mod catalog;
mod chain;
pub use chain::{RecoveryBuildStage, RecoveryChainWitness};
mod field;
mod mirror;
mod parallel;
mod parallel_dispatch;
pub use parallel_dispatch::{RecoveryBuildParallelCoordinator, RecoveryBuildParallelWorker};
mod population;
mod staged;
pub use parallel::{
    RecoveryBuildParallelError, RecoveryBuildParallelProduce, RecoveryBuildParallelProgress,
};
#[cfg(test)]
mod parallel_tests;
mod search;
#[cfg(test)]
mod tests;
pub use field::RecoveryBuildFields;
pub use population::{
    RecoveryBuildExample, RecoveryBuildPopulation, RecoveryBuildQuery, RecoveryBuildSolution,
};
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
}
