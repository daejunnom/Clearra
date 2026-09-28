//! Exact forward search for fixed-queue damage/REN and fixed/pattern spin outcomes.

mod board;
mod boundary_recovery;
mod boundary_recovery_pattern;
mod boundary_recovery_population;
mod cross_stage_catalog;
mod cross_stage_recovery;
mod parallel;
mod query;
mod reachability;
mod recovery_build;
mod result;
mod search;
mod t_spin_acceleration;

pub use boundary_recovery::{
    BoundaryRecoveryBagRoleError, BoundaryRecoveryBagRolePlan, BoundaryRecoveryError,
    BoundaryRecoveryQuery, BoundaryRecoveryReport, BoundaryRecoveryStatus, BoundaryRecoveryStep,
    EarlyPlacementPolicy, MAX_BOUNDARY_QUEUE_PIECES,
};
pub use boundary_recovery_pattern::{BoundaryRecoveryPatternError, BoundaryRecoveryPatternQuery};
pub use boundary_recovery_population::{
    search_boundary_recovery_population, BoundaryRecoveryPopulationError,
    BoundaryRecoveryPopulationLimits, BoundaryRecoveryPopulationReport,
};
pub use cross_stage_catalog::{
    CrossStageCatalog, CrossStageCatalogCompletion, CrossStageCatalogError, CrossStageCatalogQuery,
    CrossStageCatalogReport, CrossStageCatalogStatus, CrossStageCatalogWitness,
    CrossStageExecution,
};
pub use cross_stage_recovery::{
    CrossStageEarlyLimit, CrossStagePairReport, CrossStagePairStatus, CrossStageRecoveryQuery,
    CrossStageRole, CrossStageSearchError,
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

pub use recovery_build::{
    RecoveryBuildError, RecoveryBuildExample, RecoveryBuildFields, RecoveryBuildFixedQuery,
    RecoveryBuildFixedReport, RecoveryBuildParallelCoordinator, RecoveryBuildParallelError,
    RecoveryBuildParallelProduce, RecoveryBuildParallelProgress, RecoveryBuildParallelWorker,
    RecoveryBuildPopulation, RecoveryBuildQuery, RecoveryBuildSolution, RecoveryBuildStatus,
    RecoveryBuildStep,
};
