use super::*;

#[test]
fn standard_tetromino_piece_uses_stable_definition_id() {
    let piece = StandardTetrominoPiece::new(PieceKind::T);

    assert_eq!(piece.kind(), PieceKind::T);
    assert_eq!(piece.area(), 4);
    assert_eq!(piece.piece_definition_id().as_str(), "std:T");
}

#[test]
fn standard_tetromino_fast_path_unchanged_marker() {
    assert!(standard_tetromino_fast_path_unchanged());
}
