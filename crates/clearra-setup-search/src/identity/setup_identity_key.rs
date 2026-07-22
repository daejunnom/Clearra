use clearra_core_domain::piece::piece_kind::PieceKind;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SetupIdentityKey {
    occupied_shape: u64,
    hold_requirement: Option<PieceKind>,
}

impl SetupIdentityKey {
    pub fn new(occupied_shape: u64, hold_requirement: Option<PieceKind>) -> Self {
        Self {
            occupied_shape,
            hold_requirement,
        }
    }
}
impl SetupIdentityKey {
    pub fn occupied_shape(self) -> u64 {
        self.occupied_shape
    }
}
impl SetupIdentityKey {
    pub fn hold_requirement(self) -> Option<PieceKind> {
        self.hold_requirement
    }
}
