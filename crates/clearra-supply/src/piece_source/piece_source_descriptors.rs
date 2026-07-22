use clearra_core_domain::piece::piece_kind::PieceKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixedPieceSequence {
    pieces: Vec<PieceKind>,
}

impl FixedPieceSequence {
    pub fn new(pieces: Vec<PieceKind>) -> Self {
        Self { pieces }
    }
}
impl FixedPieceSequence {
    pub fn pieces(&self) -> &[PieceKind] {
        &self.pieces
    }
}
impl FixedPieceSequence {
    pub fn len(&self) -> usize {
        self.pieces.len()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BagUniverseDescriptor {
    pattern: Vec<PieceKind>,
}

impl BagUniverseDescriptor {
    pub fn new(pattern: Vec<PieceKind>) -> Self {
        Self { pattern }
    }
}
impl BagUniverseDescriptor {
    pub fn pattern(&self) -> &[PieceKind] {
        &self.pattern
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ObservedWindowDescriptor {
    observed: Vec<PieceKind>,
    budget: usize,
}

impl ObservedWindowDescriptor {
    pub fn new(observed: Vec<PieceKind>, budget: usize) -> Self {
        Self { observed, budget }
    }
}
impl ObservedWindowDescriptor {
    pub fn observed(&self) -> &[PieceKind] {
        &self.observed
    }
}
impl ObservedWindowDescriptor {
    pub fn budget(&self) -> usize {
        self.budget
    }
}
