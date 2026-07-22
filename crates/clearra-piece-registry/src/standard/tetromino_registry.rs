use crate::{
    registry::piece_registry::PieceRegistry,
    standard::tetromino_shapes::STANDARD_TETROMINO_DEFINITIONS,
};

pub fn standard_tetromino_registry() -> PieceRegistry {
    PieceRegistry::new(&STANDARD_TETROMINO_DEFINITIONS)
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::piece::piece_kind::PieceKind;

    use super::*;

    #[test]
    fn registry_contains_all_standard_pieces() {
        let registry = standard_tetromino_registry();

        assert_eq!(registry.len(), 7);
        for kind in PieceKind::STANDARD_TETROMINOES {
            assert_eq!(
                registry.get(kind).map(|definition| definition.kind()),
                Some(kind)
            );
        }
    }
}
