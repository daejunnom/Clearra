use clearra_core_domain::piece::piece_kind::PieceKind;

use super::frontier_key::FrontierKey;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrontierTransition {
    from: FrontierKey,
    to: FrontierKey,
    consumed: PieceKind,
}

impl FrontierTransition {
    pub fn new(from: FrontierKey, to: FrontierKey, consumed: PieceKind) -> Self {
        Self { from, to, consumed }
    }
}
impl FrontierTransition {
    pub fn from(&self) -> &FrontierKey {
        &self.from
    }
}
impl FrontierTransition {
    pub fn to(&self) -> &FrontierKey {
        &self.to
    }
}
impl FrontierTransition {
    pub fn consumed(&self) -> PieceKind {
        self.consumed
    }
}
