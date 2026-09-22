//! Exact forward search for fixed-queue damage/REN and fixed/pattern spin outcomes.

mod board;
mod boundary_recovery;
mod parallel;
mod query;
mod reachability;
mod result;
mod search;
mod t_spin_acceleration;

pub use boundary_recovery::{
    BoundaryRecoveryError, BoundaryRecoveryQuery, BoundaryRecoveryReport, BoundaryRecoveryStatus,
    BoundaryRecoveryStep,
};
pub use parallel::{
    ForwardParallelBatchPolicy, ForwardParallelCoordinator, ForwardParallelError,
    ForwardParallelProduce, ForwardParallelProgress, ForwardParallelWorker,
};
pub use query::{
    ForwardLineClearPolicy, ForwardPieceSource, ForwardSearchMode, ForwardSearchQuery,
    ForwardSpinCategory, ForwardSpinLineRequirement, ForwardSpinTarget,
};
pub use result::{ForwardPathStep, ForwardSearchOutcome, ForwardSearchReport, ForwardSpinGroup};
pub use search::{ForwardSearchAdvance, ForwardSearchError, ForwardSearchSession};

/// The public fixed-queue REN boundary. Larger inputs fail closed before search starts.
pub const MAX_REN_QUEUE_PIECES: usize = 22;
