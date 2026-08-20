use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_supply::hold_automaton::HoldAutomatonState;

use super::piece_source_descriptor::provenance_fingerprint;

pub const C_HOLD_TRANSITION_USE_CURRENT: u32 = 1;
pub const C_HOLD_TRANSITION_SWAP_HELD: u32 = 2;
pub const C_HOLD_TRANSITION_STORE_CURRENT_THEN_USE_NEXT: u32 = 3;

pub const C_PIECE_NONE: u8 = 0;
pub const C_PIECE_I: u8 = 1;
pub const C_PIECE_O: u8 = 2;
pub const C_PIECE_T: u8 = 3;
pub const C_PIECE_S: u8 = 4;
pub const C_PIECE_Z: u8 = 5;
pub const C_PIECE_J: u8 = 6;
pub const C_PIECE_L: u8 = 7;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CHoldAutomatonStateDescriptor {
    pub piece_source_id: u64,
    pub cursor: u16,
    pub bag_epoch: u16,
    pub bag_remainder_key: u64,
    pub provenance_id: u64,
    pub hold_piece: u8,
    pub hold_empty: u8,
    pub terminal_projection_consumed: u8,
    pub terminal_projection_provenance: u8,
    pub reserved: [u8; 4],
}

pub struct HoldAutomatonDescriptorCompiler;

impl HoldAutomatonDescriptorCompiler {
    pub fn compile(state: HoldAutomatonState) -> CHoldAutomatonStateDescriptor {
        CHoldAutomatonStateDescriptor {
            piece_source_id: state.piece_source_id.get(),
            cursor: state.cursor,
            bag_epoch: state.bag_epoch,
            bag_remainder_key: state.bag_remainder_key,
            provenance_id: u64::from(provenance_fingerprint(state.provenance.0)),
            hold_piece: state.hold_piece.map_or(C_PIECE_NONE, piece_to_c),
            hold_empty: u8::from(state.hold_empty),
            terminal_projection_consumed: 0,
            terminal_projection_provenance: 0,
            reserved: [0; 4],
        }
    }
}

fn piece_to_c(piece: PieceKind) -> u8 {
    match piece {
        PieceKind::I => C_PIECE_I,
        PieceKind::O => C_PIECE_O,
        PieceKind::T => C_PIECE_T,
        PieceKind::S => C_PIECE_S,
        PieceKind::Z => C_PIECE_Z,
        PieceKind::J => C_PIECE_J,
        PieceKind::L => C_PIECE_L,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clearra_supply::{
        hold_automaton::{HoldAutomatonState, SupplyProvenanceId},
        piece_source::PieceSourceId,
    };

    #[test]
    fn ffi_hold_automaton_state_descriptor_preserves_memo_key_fields() {
        let state = HoldAutomatonState::new(
            PieceSourceId::new(11),
            3,
            Some(PieceKind::J),
            2,
            0xfeed,
            SupplyProvenanceId(77),
        );

        let descriptor = HoldAutomatonDescriptorCompiler::compile(state);

        assert_eq!(descriptor.piece_source_id, 11);
        assert_eq!(descriptor.cursor, 3);
        assert_eq!(descriptor.bag_epoch, 2);
        assert_eq!(descriptor.bag_remainder_key, 0xfeed);
        assert_eq!(descriptor.provenance_id, 77);
        assert_eq!(descriptor.hold_piece, C_PIECE_J);
        assert_eq!(descriptor.hold_empty, 0);
    }
}
