use clearra_core_domain::{ids::setup_id::BuildVariantId, piece::piece_kind::PieceKind};
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;

use crate::{
    identity::build_identity::BuildIdentity,
    variant::{build_variant::BuildVariant, tiling_variant::TilingVariant},
};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BuildVariantEnumerator;

impl BuildVariantEnumerator {
    pub fn from_core_buildup(
        id: BuildVariantId,
        tiling: &TilingVariant,
        hold_requirement: Option<PieceKind>,
        coverage: PatternBitSet,
        proof: BuildUpVariantProof,
    ) -> Option<BuildVariant> {
        (proof.has_build_variant() && proof.matches_tiling(tiling, hold_requirement)).then(|| {
            BuildVariant::new(
                id,
                tiling.id(),
                BuildIdentity::new(tiling.occupied_shape(), hold_requirement),
                coverage,
            )
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildUpVariantProof {
    successful_build_variant_count: usize,
    coverage_row_count: usize,
    occupied_shape: Option<u64>,
    hold_requirement: Option<PieceKind>,
    placed_piece_count: Option<usize>,
}

impl BuildUpVariantProof {
    #[cfg(test)]
    pub(crate) fn new(successful_build_variant_count: usize, coverage_row_count: usize) -> Self {
        Self {
            successful_build_variant_count,
            coverage_row_count,
            occupied_shape: None,
            hold_requirement: None,
            placed_piece_count: None,
        }
    }
}
impl BuildUpVariantProof {
    #[cfg(test)]
    pub(crate) fn with_build_input(
        mut self,
        occupied_shape: u64,
        hold_requirement: Option<PieceKind>,
        placed_piece_count: usize,
    ) -> Self {
        self.occupied_shape = Some(occupied_shape);
        self.hold_requirement = hold_requirement;
        self.placed_piece_count = Some(placed_piece_count);
        self
    }
}
impl BuildUpVariantProof {
    pub fn successful_build_variant_count(self) -> usize {
        self.successful_build_variant_count
    }
}
impl BuildUpVariantProof {
    pub fn coverage_row_count(self) -> usize {
        self.coverage_row_count
    }
}
impl BuildUpVariantProof {
    pub fn has_build_variant(self) -> bool {
        self.successful_build_variant_count > 0 && self.coverage_row_count > 0
    }
}
impl BuildUpVariantProof {
    fn matches_tiling(self, tiling: &TilingVariant, hold_requirement: Option<PieceKind>) -> bool {
        self.occupied_shape == Some(tiling.occupied_shape())
            && self.hold_requirement == hold_requirement
            && self.placed_piece_count == Some(tiling.pieces().len())
    }
}

#[cfg(test)]
#[path = "build_variant_enumerator_tests.rs"]
mod tests;
