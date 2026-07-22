use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ShapeCell {
    x: i8,
    y: i8,
}

impl ShapeCell {
    pub const fn new(x: i8, y: i8) -> Self {
        Self { x, y }
    }
}
impl ShapeCell {
    pub fn x(self) -> i8 {
        self.x
    }
}
impl ShapeCell {
    pub fn y(self) -> i8 {
        self.y
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PieceRotationShape {
    cells: [ShapeCell; 4],
}

impl PieceRotationShape {
    pub const fn new(cells: [ShapeCell; 4]) -> Self {
        Self { cells }
    }
}
impl PieceRotationShape {
    pub fn cells(self) -> [ShapeCell; 4] {
        self.cells
    }
}
impl PieceRotationShape {
    pub fn width(self) -> u8 {
        self.cells
            .iter()
            .map(|cell| cell.x)
            .max()
            .map_or(0, |max_x| (max_x + 1) as u8)
    }
}
impl PieceRotationShape {
    pub fn height(self) -> u8 {
        self.cells
            .iter()
            .map(|cell| cell.y)
            .max()
            .map_or(0, |max_y| (max_y + 1) as u8)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PieceDefinition {
    kind: PieceKind,
    rotations: [PieceRotationShape; 4],
}

impl PieceDefinition {
    pub const fn new(kind: PieceKind, rotations: [PieceRotationShape; 4]) -> Self {
        Self { kind, rotations }
    }
}
impl PieceDefinition {
    pub fn kind(self) -> PieceKind {
        self.kind
    }
}
impl PieceDefinition {
    pub fn rotations(self) -> [PieceRotationShape; 4] {
        self.rotations
    }
}
impl PieceDefinition {
    pub fn shape(self, rotation: RotationState) -> PieceRotationShape {
        self.rotations[usize::from(rotation.quarter_turns())]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PieceRegistry {
    definitions: &'static [PieceDefinition],
}

impl PieceRegistry {
    pub const fn new(definitions: &'static [PieceDefinition]) -> Self {
        Self { definitions }
    }
}
impl PieceRegistry {
    pub fn definitions(self) -> &'static [PieceDefinition] {
        self.definitions
    }
}
impl PieceRegistry {
    pub fn len(self) -> usize {
        self.definitions.len()
    }
}
impl PieceRegistry {
    pub fn is_empty(self) -> bool {
        self.definitions.is_empty()
    }
}
impl PieceRegistry {
    pub fn get(self, kind: PieceKind) -> Option<PieceDefinition> {
        self.definitions
            .iter()
            .copied()
            .find(|definition| definition.kind == kind)
    }
}
