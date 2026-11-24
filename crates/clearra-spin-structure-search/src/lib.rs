//! Exact, unordered-inventory search for minimum spin structures.
//!
//! This crate deliberately has no dependency on the fixed-source forward
//! search.  It searches a different contract: pieces are supplied as a
//! multiset, every reported structure has an exact reachable build witness,
//! and subset-minimal results are retained across every searched depth.

mod board;
mod build;
mod cooperative_sort;
mod source_parallel;
mod source_worker;
pub use source_parallel::{
    SpinStructureParallelCoordinator, SpinStructureParallelError, SpinStructureParallelProduce,
    SpinStructureParallelProgress, SpinStructureParallelResult, SpinStructureParallelWorker,
};
pub use source_worker::{
    SpinStructureSourceCompletion, SpinStructureSourceMergeCursor, SpinStructureSourceMerger,
    SpinStructureSourcePlan, SpinStructureSourcePreparation, SpinStructureSourceWorker,
};
mod corner;
mod entry;
mod fill;
mod logical;
mod minimal;
mod model;
mod operation_catalog;
mod order_coverage;
mod order_parallel;
pub use order_parallel::SpinStructureParallelMode;
mod stage_timer;
mod structural_expand;
mod structural_fill;
mod structural_search;
mod structural_verify;
mod support;
mod verify;

pub use board::StructureBoard;
pub use build::{
    AllMiniPlusStructureSearch, AllMiniStructureSearch, AllSpinPlusStructureSearch,
    AllSpinStructureSearch, SpinStructureSearch, SpinStructureSearcher, TSpinPlusStructureSearch,
    TSpinStructureSearch,
};
pub use model::{
    LayerMetrics, MinimalityPolicy, PieceInventory, SpinLineRequirement, SpinStructureError,
    SpinStructureMode, SpinStructureOutcome, SpinStructureQuery, SpinStructureReport,
    SpinStructureStageMetrics, SpinStructureTask, SpinStructureTimingMetrics, StructureOperation,
    StructurePlacement,
};
pub use order_coverage::{
    analyze_spin_structure_coverage, analyze_spin_structure_coverage_with_control,
    guaranteed_spin_structure_family, guaranteed_spin_structure_family_with_control,
    InventoryOrderSource, SpinStructureCoverageAnalysis, SpinStructureCoverageRow,
    SpinStructureOrderCoverageError, DEFAULT_SPIN_STRUCTURE_MAX_PATTERNS,
};
