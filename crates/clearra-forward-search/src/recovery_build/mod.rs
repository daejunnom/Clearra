//! Stage-factorized paired Build coverage with demand-driven boundary repair.
//! The original weighted supply universe is preserved without pair expansion.
mod catalog;
mod field;
mod parallel;
mod population;
mod staged;
pub use parallel::{
    RecoveryBuildParallelCoordinator, RecoveryBuildParallelError, RecoveryBuildParallelProduce,
    RecoveryBuildParallelProgress, RecoveryBuildParallelWorker,
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
