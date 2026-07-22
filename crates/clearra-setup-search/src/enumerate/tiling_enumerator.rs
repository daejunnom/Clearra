use clearra_core_domain::{ids::setup_id::TilingVariantId, piece::piece_kind::PieceKind};

use crate::{identity::shape_family::ShapeFamily, variant::tiling_variant::TilingVariant};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TilingEnumerator;

impl TilingEnumerator {
    pub fn single_tiling(
        id: TilingVariantId,
        family: ShapeFamily,
        pieces: Vec<PieceKind>,
    ) -> TilingVariant {
        TilingVariant::new(id, family.id(), family.occupied_shape(), pieces)
    }
}
