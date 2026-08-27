use core::fmt;

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use fumen::{CellColor, PieceType};

use super::source_fumen_diagram::{decode_document, SourceFumenDiagramError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceFumenDocumentOperation {
    pub board_before: u64,
    pub piece: PieceKind,
    pub rotation: RotationState,
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceFumenOperationDocument {
    pub width: u8,
    pub height: u8,
    pub operations: Vec<SourceFumenDocumentOperation>,
}

impl SourceFumenOperationDocument {
    pub fn decode(source: &str) -> Result<Self, SourceFumenOperationDocumentError> {
        let document =
            decode_document(source).map_err(SourceFumenOperationDocumentError::Decode)?;
        if document.pages.is_empty() {
            return Err(SourceFumenOperationDocumentError::MissingConcreteOperations);
        }
        let mut operations = Vec::new();
        operations
            .try_reserve(document.pages.len())
            .map_err(|_| SourceFumenOperationDocumentError::CapacityExceeded)?;
        let mut height = 4_u8;
        for (page_index, page) in document.pages.iter().enumerate() {
            if !page.lock
                || page.rise
                || page.mirror
                || page
                    .garbage_row
                    .iter()
                    .any(|cell| *cell != CellColor::Empty)
                || page
                    .comment
                    .as_deref()
                    .is_some_and(|comment| comment.starts_with("#Q="))
            {
                return Err(
                    SourceFumenOperationDocumentError::UnsupportedPageSemantics { page_index },
                );
            }
            let piece = page
                .piece
                .ok_or(SourceFumenOperationDocumentError::MissingOperation { page_index })?;
            let mut board_before = 0_u64;
            for (y, row) in page.field.iter().enumerate() {
                for (x, cell) in row.iter().enumerate() {
                    if *cell == CellColor::Empty {
                        continue;
                    }
                    let bit = y * 10 + x;
                    if bit >= 64 {
                        return Err(SourceFumenOperationDocumentError::CellOutsideBoard64 {
                            page_index,
                            x,
                            y,
                        });
                    }
                    board_before |= 1_u64 << bit;
                    height = height.max((y + 1) as u8);
                }
            }
            let piece_kind = piece_kind(piece.kind);
            let rotation = rotation_state(piece.rotation);
            for (offset_x, offset_y) in centered_offsets(piece_kind, rotation) {
                let cell_x = i64::from(piece.x) + i64::from(offset_x);
                let cell_y = i64::from(piece.y) + i64::from(offset_y);
                if !(0..10).contains(&cell_x) || !(0..6).contains(&cell_y) {
                    return Err(SourceFumenOperationDocumentError::OperationOutsideBoard64 {
                        page_index,
                    });
                }
                height = height.max((cell_y + 1) as u8);
            }
            operations.push(SourceFumenDocumentOperation {
                board_before,
                piece: piece_kind,
                rotation,
                x: i32::try_from(piece.x).map_err(|_| {
                    SourceFumenOperationDocumentError::OperationOutsideBoard64 { page_index }
                })?,
                y: i32::try_from(piece.y).map_err(|_| {
                    SourceFumenOperationDocumentError::OperationOutsideBoard64 { page_index }
                })?,
            });
        }
        Ok(Self {
            width: 10,
            height,
            operations,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SourceFumenOperationDocumentError {
    Decode(SourceFumenDiagramError),
    MissingConcreteOperations,
    MissingOperation {
        page_index: usize,
    },
    UnsupportedPageSemantics {
        page_index: usize,
    },
    CellOutsideBoard64 {
        page_index: usize,
        x: usize,
        y: usize,
    },
    OperationOutsideBoard64 {
        page_index: usize,
    },
    CapacityExceeded,
}

impl fmt::Display for SourceFumenOperationDocumentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}
impl std::error::Error for SourceFumenOperationDocumentError {}

const fn piece_kind(piece: PieceType) -> PieceKind {
    match piece {
        PieceType::I => PieceKind::I,
        PieceType::O => PieceKind::O,
        PieceType::T => PieceKind::T,
        PieceType::S => PieceKind::S,
        PieceType::Z => PieceKind::Z,
        PieceType::J => PieceKind::J,
        PieceType::L => PieceKind::L,
    }
}
const fn rotation_state(rotation: fumen::RotationState) -> RotationState {
    match rotation {
        fumen::RotationState::South => RotationState::Zero,
        fumen::RotationState::East => RotationState::Right,
        fumen::RotationState::North => RotationState::Two,
        fumen::RotationState::West => RotationState::Left,
    }
}
fn centered_offsets(piece: PieceKind, rotation: RotationState) -> [(i32, i32); 4] {
    let mut offsets = match piece {
        PieceKind::I => [(-1, 0), (0, 0), (1, 0), (2, 0)],
        PieceKind::O => [(0, 0), (1, 0), (0, 1), (1, 1)],
        PieceKind::T => [(-1, 0), (0, 0), (1, 0), (0, 1)],
        PieceKind::S => [(-1, 0), (0, 0), (0, 1), (1, 1)],
        PieceKind::Z => [(-1, 1), (0, 1), (0, 0), (1, 0)],
        PieceKind::J => [(-1, 1), (-1, 0), (0, 0), (1, 0)],
        PieceKind::L => [(1, 1), (-1, 0), (0, 0), (1, 0)],
    };
    if piece != PieceKind::O {
        for _ in 0..rotation.quarter_turns() {
            for (x, y) in &mut offsets {
                (*x, *y) = (*y, -*x);
            }
        }
    }
    offsets
}
