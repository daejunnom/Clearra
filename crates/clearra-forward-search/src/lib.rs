//! Exact forward search for fixed-queue damage and fixed/pattern spin outcomes.

mod board;
mod parallel;
mod query;
mod reachability;
mod result;
mod search;
mod t_spin_acceleration;

pub use parallel::{
    ForwardParallelCoordinator, ForwardParallelError, ForwardParallelProduce,
    ForwardParallelProgress, ForwardParallelWorker,
};
pub use query::{
    ForwardPieceSource, ForwardSearchMode, ForwardSearchQuery, ForwardSpinCategory,
    ForwardSpinTarget,
};
pub use result::{ForwardPathStep, ForwardSearchOutcome, ForwardSearchReport, ForwardSpinGroup};
pub use search::{ForwardSearchAdvance, ForwardSearchError, ForwardSearchSession};
