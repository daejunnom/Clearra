use crate::{
    operation::operation::{Operation, OperationId},
    piece::piece_kind::PieceKind,
};

use super::shape_family::ShapeFamilyId;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TilingVariantId(u32);

impl TilingVariantId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}
impl TilingVariantId {
    pub const fn get(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TilingKey(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CellPartitionKey(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PieceCountVector {
    counts: [u8; 7],
}

impl PieceCountVector {
    pub const fn empty() -> Self {
        Self { counts: [0; 7] }
    }
}
impl PieceCountVector {
    pub fn from_pieces(pieces: &[PieceKind]) -> Self {
        let mut vector = Self::empty();
        for piece in pieces {
            vector.increment(*piece);
        }
        vector
    }
}
impl PieceCountVector {
    pub fn increment(&mut self, piece: PieceKind) {
        self.counts[piece_index(piece)] = self.counts[piece_index(piece)].saturating_add(1);
    }
}
impl PieceCountVector {
    pub const fn counts(self) -> [u8; 7] {
        self.counts
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OperationPlacement {
    pub operation_id: OperationId,
    pub piece: PieceKind,
    pub cells_mask: u64,
    pub x: i8,
    pub y: i8,
}

impl OperationPlacement {
    pub const fn new(
        operation_id: OperationId,
        piece: PieceKind,
        cells_mask: u64,
        x: i8,
        y: i8,
    ) -> Self {
        Self {
            operation_id,
            piece,
            cells_mask,
            x,
            y,
        }
    }
}

impl From<Operation> for OperationPlacement {
    fn from(operation: Operation) -> Self {
        Self {
            operation_id: operation.operation_id,
            piece: operation.piece,
            cells_mask: operation.cells_mask,
            x: operation.x,
            y: operation.y,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TilingVariant {
    pub tiling_variant_id: TilingVariantId,
    pub shape_family_id: ShapeFamilyId,
    pub piece_multiset: PieceCountVector,
    pub placements: Vec<OperationPlacement>,
    pub tiling_key: TilingKey,
}

impl TilingVariant {
    pub fn new(
        tiling_variant_id: TilingVariantId,
        shape_family_id: ShapeFamilyId,
        piece_multiset: PieceCountVector,
        placements: Vec<OperationPlacement>,
        tiling_key: TilingKey,
    ) -> Self {
        Self {
            tiling_variant_id,
            shape_family_id,
            piece_multiset,
            placements,
            tiling_key,
        }
    }
}

fn piece_index(piece: PieceKind) -> usize {
    match piece {
        PieceKind::I => 0,
        PieceKind::O => 1,
        PieceKind::T => 2,
        PieceKind::S => 3,
        PieceKind::Z => 4,
        PieceKind::J => 5,
        PieceKind::L => 6,
    }
}
