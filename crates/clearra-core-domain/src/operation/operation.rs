use crate::piece::{piece_kind::PieceKind, rotation::RotationState};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OperationId(pub u16);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoordinateFrame {
    TargetFrame,
    LockFrame,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Operation {
    pub operation_id: OperationId,
    pub piece: PieceKind,
    pub rotation: RotationState,
    pub x: i8,
    pub y: i8,
    pub cells_mask: u64,
    pub coordinate_frame: CoordinateFrame,
}

impl Operation {
    pub fn target_frame(
        operation_id: OperationId,
        piece: PieceKind,
        rotation: RotationState,
        x: i8,
        y: i8,
        cells_mask: u64,
    ) -> Self {
        Self {
            operation_id,
            piece,
            rotation,
            x,
            y,
            cells_mask,
            coordinate_frame: CoordinateFrame::TargetFrame,
        }
    }
}
impl Operation {
    pub fn with_lock_frame_y(mut self, y: i8, cells_mask: u64) -> Self {
        self.y = y;
        self.cells_mask = cells_mask;
        self.coordinate_frame = CoordinateFrame::LockFrame;
        self
    }
}
impl Operation {
    pub fn is_target_frame(self) -> bool {
        self.coordinate_frame == CoordinateFrame::TargetFrame
    }
}
impl Operation {
    pub fn is_lock_frame(self) -> bool {
        self.coordinate_frame == CoordinateFrame::LockFrame
    }
}

#[cfg(test)]
#[path = "operation_tests.rs"]
mod tests;
