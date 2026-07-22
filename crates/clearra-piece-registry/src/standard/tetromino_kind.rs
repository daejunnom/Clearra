use clearra_core_domain::piece::piece_kind::PieceKind;

pub fn is_standard_tetromino(kind: PieceKind) -> bool {
    PieceKind::STANDARD_TETROMINOES.contains(&kind)
}
