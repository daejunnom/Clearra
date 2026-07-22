use clearra_core_domain::{
    ids::setup_id::{SetupFamilyId, TilingVariantId},
    piece::piece_kind::PieceKind,
    solution::{OperationPlacement, PieceCountVector, TilingKey},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TilingVariant {
    id: TilingVariantId,
    family_id: SetupFamilyId,
    occupied_shape: u64,
    piece_multiset: PieceCountVector,
    placements: Vec<OperationPlacement>,
    tiling_key: TilingKey,
    pieces: Vec<PieceKind>,
}

impl TilingVariant {
    pub fn new(
        id: TilingVariantId,
        family_id: SetupFamilyId,
        occupied_shape: u64,
        pieces: Vec<PieceKind>,
    ) -> Self {
        let piece_multiset = PieceCountVector::from_pieces(&pieces);
        Self {
            id,
            family_id,
            occupied_shape,
            piece_multiset,
            placements: Vec::new(),
            tiling_key: TilingKey(occupied_shape),
            pieces,
        }
    }
}
impl TilingVariant {
    pub fn with_placements_and_tiling_key(
        mut self,
        placements: Vec<OperationPlacement>,
        tiling_key: TilingKey,
    ) -> Self {
        self.placements = placements;
        self.tiling_key = tiling_key;
        self
    }
}
impl TilingVariant {
    pub fn id(&self) -> TilingVariantId {
        self.id
    }
}
impl TilingVariant {
    pub fn family_id(&self) -> SetupFamilyId {
        self.family_id
    }
}
impl TilingVariant {
    pub fn occupied_shape(&self) -> u64 {
        self.occupied_shape
    }
}
impl TilingVariant {
    pub fn pieces(&self) -> &[PieceKind] {
        &self.pieces
    }
}
impl TilingVariant {
    pub fn piece_multiset(&self) -> PieceCountVector {
        self.piece_multiset
    }
}
impl TilingVariant {
    pub fn placements(&self) -> &[OperationPlacement] {
        &self.placements
    }
}
impl TilingVariant {
    pub fn tiling_key(&self) -> TilingKey {
        self.tiling_key
    }
}
