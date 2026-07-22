use clearra_core_domain::piece::piece_kind::PieceKind;

use super::setup_identity_key::SetupIdentityKey;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildIdentity {
    key: SetupIdentityKey,
}

impl BuildIdentity {
    pub fn new(occupied_shape: u64, hold_requirement: Option<PieceKind>) -> Self {
        Self {
            key: SetupIdentityKey::new(occupied_shape, hold_requirement),
        }
    }
}
impl BuildIdentity {
    pub fn key(self) -> SetupIdentityKey {
        self.key
    }
}
impl BuildIdentity {
    pub fn occupied_shape(self) -> u64 {
        self.key.occupied_shape()
    }
}
impl BuildIdentity {
    pub fn hold_requirement(self) -> Option<PieceKind> {
        self.key.hold_requirement()
    }
}
