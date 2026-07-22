use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::registry::piece_registry::{PieceDefinition, PieceRotationShape, ShapeCell};

const fn c(x: i8, y: i8) -> ShapeCell {
    ShapeCell::new(x, y)
}

const fn s(cells: [ShapeCell; 4]) -> PieceRotationShape {
    PieceRotationShape::new(cells)
}

pub const I_DEFINITION: PieceDefinition = PieceDefinition::new(
    PieceKind::I,
    [
        s([c(0, 0), c(1, 0), c(2, 0), c(3, 0)]),
        s([c(0, 0), c(0, 1), c(0, 2), c(0, 3)]),
        s([c(0, 0), c(1, 0), c(2, 0), c(3, 0)]),
        s([c(0, 0), c(0, 1), c(0, 2), c(0, 3)]),
    ],
);

pub const O_DEFINITION: PieceDefinition = PieceDefinition::new(
    PieceKind::O,
    [
        s([c(0, 0), c(1, 0), c(0, 1), c(1, 1)]),
        s([c(0, 0), c(1, 0), c(0, 1), c(1, 1)]),
        s([c(0, 0), c(1, 0), c(0, 1), c(1, 1)]),
        s([c(0, 0), c(1, 0), c(0, 1), c(1, 1)]),
    ],
);

pub const T_DEFINITION: PieceDefinition = PieceDefinition::new(
    PieceKind::T,
    [
        s([c(0, 0), c(1, 0), c(2, 0), c(1, 1)]),
        s([c(0, 0), c(0, 1), c(0, 2), c(1, 1)]),
        s([c(0, 1), c(1, 1), c(2, 1), c(1, 0)]),
        s([c(1, 0), c(1, 1), c(1, 2), c(0, 1)]),
    ],
);

pub const S_DEFINITION: PieceDefinition = PieceDefinition::new(
    PieceKind::S,
    [
        s([c(0, 0), c(1, 0), c(1, 1), c(2, 1)]),
        s([c(1, 0), c(0, 1), c(1, 1), c(0, 2)]),
        s([c(0, 0), c(1, 0), c(1, 1), c(2, 1)]),
        s([c(1, 0), c(0, 1), c(1, 1), c(0, 2)]),
    ],
);

pub const Z_DEFINITION: PieceDefinition = PieceDefinition::new(
    PieceKind::Z,
    [
        s([c(1, 0), c(2, 0), c(0, 1), c(1, 1)]),
        s([c(0, 0), c(0, 1), c(1, 1), c(1, 2)]),
        s([c(1, 0), c(2, 0), c(0, 1), c(1, 1)]),
        s([c(0, 0), c(0, 1), c(1, 1), c(1, 2)]),
    ],
);

pub const J_DEFINITION: PieceDefinition = PieceDefinition::new(
    PieceKind::J,
    [
        s([c(0, 1), c(0, 0), c(1, 0), c(2, 0)]),
        s([c(0, 0), c(0, 1), c(0, 2), c(1, 2)]),
        s([c(0, 1), c(1, 1), c(2, 1), c(2, 0)]),
        s([c(0, 0), c(1, 0), c(1, 1), c(1, 2)]),
    ],
);

pub const L_DEFINITION: PieceDefinition = PieceDefinition::new(
    PieceKind::L,
    [
        s([c(2, 1), c(0, 0), c(1, 0), c(2, 0)]),
        s([c(0, 0), c(0, 1), c(0, 2), c(1, 0)]),
        s([c(0, 1), c(1, 1), c(2, 1), c(0, 0)]),
        s([c(0, 2), c(1, 0), c(1, 1), c(1, 2)]),
    ],
);

pub const STANDARD_TETROMINO_DEFINITIONS: [PieceDefinition; 7] = [
    I_DEFINITION,
    O_DEFINITION,
    T_DEFINITION,
    S_DEFINITION,
    Z_DEFINITION,
    J_DEFINITION,
    L_DEFINITION,
];

#[cfg(test)]
#[path = "tetromino_shapes_tests.rs"]
mod tests;
