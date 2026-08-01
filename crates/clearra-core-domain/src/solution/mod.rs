pub mod build_variant;
pub mod normalized_tiling_solution;
pub mod shape_family;
pub mod tiling_variant;

pub use build_variant::{
    BuildVariant, BuildVariantId, HoldDecision, LineClearEvent, OperationSetKey, PatternId,
    ReachabilityEvidence,
};
pub use normalized_tiling_solution::{
    normalized_tiling_solution_key_set_hash_from_sorted_strings,
    normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities,
    NormalizedTilingSolutionError, NormalizedTilingSolutionKey, NormalizedTilingSolutionSet,
    NormalizedTilingSolutionSetHasher, PiecePlacementMask, StandardBoard64TilingIdentity,
    NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM, NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
    STANDARD_BOARD64_TILING_MAX_PLACEMENTS,
};
pub use shape_family::{ShapeFamily, ShapeFamilyId, ShapeKey, VisualGroupKey};
pub use tiling_variant::{
    CellPartitionKey, OperationPlacement, PieceCountVector, TilingKey, TilingVariant,
    TilingVariantId,
};

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
