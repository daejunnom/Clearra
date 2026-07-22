use super::*;

#[test]
fn parses_standard_piece_kinds_case_insensitively() {
    assert_eq!(PieceKind::from_ascii('i'), Ok(PieceKind::I));
    assert_eq!(PieceKind::from_ascii('L'), Ok(PieceKind::L));
}

#[test]
fn rejects_unknown_piece_kind() {
    assert_eq!(PieceKind::from_ascii('X'), Err(UnknownPieceKind));
}
