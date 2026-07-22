use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::piece_source::PieceSourceId;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SupplyProvenanceId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HoldAutomatonState {
    pub piece_source_id: PieceSourceId,
    pub cursor: u16,
    pub hold_piece: Option<PieceKind>,
    pub hold_empty: bool,
    pub bag_epoch: u16,
    pub bag_remainder_key: u64,
    pub provenance: SupplyProvenanceId,
}

impl HoldAutomatonState {
    pub fn new(
        piece_source_id: PieceSourceId,
        cursor: u16,
        hold_piece: Option<PieceKind>,
        bag_epoch: u16,
        bag_remainder_key: u64,
        provenance: SupplyProvenanceId,
    ) -> Self {
        Self {
            piece_source_id,
            cursor,
            hold_piece,
            hold_empty: hold_piece.is_none(),
            bag_epoch,
            bag_remainder_key,
            provenance,
        }
    }
}
impl HoldAutomatonState {
    pub fn memo_key(self) -> HoldAutomatonMemoKey {
        HoldAutomatonMemoKey {
            piece_source_id: self.piece_source_id,
            cursor: self.cursor,
            hold_piece: self.hold_piece,
            hold_empty: self.hold_empty,
            bag_epoch: self.bag_epoch,
            bag_remainder_key: self.bag_remainder_key,
            provenance: self.provenance,
        }
    }
}
impl HoldAutomatonState {
    pub const fn piece_source_id(self) -> PieceSourceId {
        self.piece_source_id
    }
}
impl HoldAutomatonState {
    pub const fn cursor(self) -> u16 {
        self.cursor
    }
}
impl HoldAutomatonState {
    pub const fn hold_piece(self) -> Option<PieceKind> {
        self.hold_piece
    }
}
impl HoldAutomatonState {
    pub const fn hold_empty(self) -> bool {
        self.hold_empty
    }
}
impl HoldAutomatonState {
    pub const fn bag_epoch(self) -> u16 {
        self.bag_epoch
    }
}
impl HoldAutomatonState {
    pub const fn bag_remainder_key(self) -> u64 {
        self.bag_remainder_key
    }
}
impl HoldAutomatonState {
    pub const fn provenance(self) -> SupplyProvenanceId {
        self.provenance
    }
}
impl HoldAutomatonState {
    pub fn apply(
        self,
        transition: HoldTransition,
        current_piece: PieceKind,
        next_piece: Option<PieceKind>,
    ) -> Result<HoldAutomatonStep, HoldTransitionError> {
        match transition {
            HoldTransition::UseCurrent => Ok(HoldAutomatonStep {
                used_piece: current_piece,
                next_state: Self {
                    cursor: self.cursor.saturating_add(1),
                    ..self
                },
            }),
            HoldTransition::SwapHeld => {
                let held = self
                    .hold_piece
                    .ok_or(HoldTransitionError::MissingHeldPiece)?;
                Ok(HoldAutomatonStep {
                    used_piece: held,
                    next_state: Self {
                        cursor: self.cursor.saturating_add(1),
                        hold_piece: Some(current_piece),
                        hold_empty: false,
                        ..self
                    },
                })
            }
            HoldTransition::StoreCurrentThenUseNext => {
                let next_piece = next_piece.ok_or(HoldTransitionError::MissingNextPiece)?;
                Ok(HoldAutomatonStep {
                    used_piece: next_piece,
                    next_state: Self {
                        cursor: self.cursor.saturating_add(2),
                        hold_piece: Some(current_piece),
                        hold_empty: false,
                        ..self
                    },
                })
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HoldTransition {
    UseCurrent,
    SwapHeld,
    StoreCurrentThenUseNext,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HoldTransitionError {
    MissingHeldPiece,
    MissingNextPiece,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HoldAutomatonStep {
    pub used_piece: PieceKind,
    pub next_state: HoldAutomatonState,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct HoldAutomatonMemoKey {
    pub piece_source_id: PieceSourceId,
    pub cursor: u16,
    pub hold_piece: Option<PieceKind>,
    pub hold_empty: bool,
    pub bag_epoch: u16,
    pub bag_remainder_key: u64,
    pub provenance: SupplyProvenanceId,
}

#[cfg(test)]
#[path = "hold_automaton_tests.rs"]
mod tests;
