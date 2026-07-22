use clearra_core_domain::piece::piece_kind::PieceKind;

use super::piece_set_profile::{PieceSetProfile, PieceSetProfileId};

pub const STANDARD_TETROMINOES: [PieceKind; 7] = PieceKind::STANDARD_TETROMINOES;

pub fn standard_tetromino_piece_set_profile() -> PieceSetProfile {
    PieceSetProfile::new(
        PieceSetProfileId::StandardTetrominoes,
        &STANDARD_TETROMINOES,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_piece_set_contains_seven_unique_pieces() {
        let profile = standard_tetromino_piece_set_profile();

        assert_eq!(profile.len(), 7);
        assert_eq!(profile.pieces(), &PieceKind::STANDARD_TETROMINOES);
    }
}
