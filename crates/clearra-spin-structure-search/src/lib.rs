//! Exact, unordered-inventory search for minimum spin structures.
//!
//! This crate deliberately has no dependency on the fixed-source forward
//! search.  It searches a different contract: pieces are supplied as a
//! multiset, every reported structure has an exact reachable build witness,
//! and subset-minimal results are retained across every searched depth.

mod board;
mod build;
mod corner;
mod entry;
mod fill;
mod logical;
mod minimal;
mod model;
mod operation_catalog;
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
