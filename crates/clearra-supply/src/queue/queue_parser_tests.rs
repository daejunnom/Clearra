use clearra_core_domain::piece::piece_kind::PieceKind;

use super::*;

#[test]
fn parses_piece_sequences_with_separators() {
    assert_eq!(
        parse_piece_sequence("I,O T-S_Z|J L"),
        Ok(vec![
            PieceKind::I,
            PieceKind::O,
            PieceKind::T,
            PieceKind::S,
            PieceKind::Z,
            PieceKind::J,
            PieceKind::L
        ])
    );
}

#[test]
fn reports_unknown_piece_position() {
    assert_eq!(
        parse_piece_sequence("IX"),
        Err(QueueParseError::UnknownPiece {
            index: 1,
            value: 'X'
        })
    );
}
