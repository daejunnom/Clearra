use clearra_core_domain::ids::setup_id::SetupFamilyId;
use clearra_coverage::{
    matrix::coverage_matrix::{CoverageMatrixError, TypedCoverageMatrix},
    pattern::pattern_bitset::PatternBitSet,
    row::{coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind},
    universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    },
};

use crate::{identity::shape_family::ShapeFamily, variant::build_variant::BuildVariant};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupCoverageBuilder {
    family: ShapeFamily,
    pattern_count: usize,
    piece_source_id: u64,
    pattern_universe_id: PatternUniverseId,
    pattern_weight_model_id: PatternWeightModelId,
    rows: Vec<CoverageRow>,
}

impl SetupCoverageBuilder {
    pub fn new(family: ShapeFamily, pattern_count: usize) -> Self {
        let pattern_universe_id = PatternUniverseId::new(stable_nonzero_identity(&format!(
            "clearra:setup-family-universe:{}:{}",
            family.id().get(),
            pattern_count
        )));
        let pattern_weight_model_id = PatternWeightModelId::new(stable_nonzero_identity(&format!(
            "clearra:setup-family-weight-model:{}:{}",
            family.id().get(),
            pattern_count
        )));
        let piece_source_id = stable_nonzero_identity(&format!(
            "clearra:setup-family-piece-source:{}:{}",
            family.id().get(),
            pattern_count
        ));
        Self {
            family,
            pattern_count,
            piece_source_id,
            pattern_universe_id,
            pattern_weight_model_id,
            rows: Vec::new(),
        }
    }
}
impl SetupCoverageBuilder {
    pub fn push_variant(
        &mut self,
        variant: &BuildVariant,
    ) -> Result<(), SetupCoverageBuilderError> {
        let actual_occupied_shape = variant.identity().occupied_shape();
        let expected_occupied_shape = self.family.occupied_shape();
        if actual_occupied_shape != expected_occupied_shape {
            return Err(SetupCoverageBuilderError::VariantShapeMismatch {
                family_id: self.family.id(),
                expected_occupied_shape,
                actual_occupied_shape,
            });
        }

        self.rows.push(CoverageRow::new_with_piece_source(
            u64::from(variant.id().get()),
            CoverageRowKind::Setup,
            self.piece_source_id,
            self.pattern_universe_id,
            self.pattern_weight_model_id,
            variant.coverage().clone(),
        ));
        Ok(())
    }
}
impl SetupCoverageBuilder {
    pub fn push_raw(&mut self, candidate_id: usize, coverage: PatternBitSet) {
        self.rows.push(CoverageRow::new_with_piece_source(
            candidate_id as u64,
            CoverageRowKind::Setup,
            self.piece_source_id,
            self.pattern_universe_id,
            self.pattern_weight_model_id,
            coverage,
        ));
    }
}
impl SetupCoverageBuilder {
    pub fn family_id(&self) -> SetupFamilyId {
        self.family.id()
    }
}
impl SetupCoverageBuilder {
    pub fn occupied_shape(&self) -> u64 {
        self.family.occupied_shape()
    }
}
impl SetupCoverageBuilder {
    pub fn build(self) -> Result<TypedCoverageMatrix, CoverageMatrixError> {
        TypedCoverageMatrix::from_rows(
            CoverageRowKind::Setup,
            self.pattern_universe_id,
            self.pattern_weight_model_id,
            self.pattern_count,
            self.rows,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupCoverageBuilderError {
    VariantShapeMismatch {
        family_id: SetupFamilyId,
        expected_occupied_shape: u64,
        actual_occupied_shape: u64,
    },
}

fn stable_nonzero_identity(material: &str) -> u64 {
    const FNV_OFFSET: u64 = 14_695_981_039_346_656_037;
    const FNV_PRIME: u64 = 1_099_511_628_211;

    let mut hash = FNV_OFFSET;
    for byte in material.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    if hash == 0 {
        1
    } else {
        hash
    }
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        ids::setup_id::{BuildVariantId, SetupFamilyId, TilingVariantId},
        piece::piece_kind::PieceKind,
    };
    use clearra_coverage::pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId};

    use crate::identity::build_identity::BuildIdentity;

    use super::*;

    #[test]
    fn rejects_variant_from_different_occupied_shape() {
        let family = ShapeFamily::new(SetupFamilyId::new(1), 0b0011);
        let mut builder = SetupCoverageBuilder::new(family, 2);
        let variant = BuildVariant::new(
            BuildVariantId::new(10),
            TilingVariantId::new(20),
            BuildIdentity::new(0b1100, Some(PieceKind::I)),
            PatternBitSet::from_patterns(2, [PatternId::new(0)]).expect("coverage"),
        );

        assert_eq!(
            builder.push_variant(&variant),
            Err(SetupCoverageBuilderError::VariantShapeMismatch {
                family_id: SetupFamilyId::new(1),
                expected_occupied_shape: 0b0011,
                actual_occupied_shape: 0b1100
            })
        );
    }
}
