use clearra_profiles::pieces::piece_set_profile::{PieceSetProfile, PieceSetProfileId};

use crate::{
    registry::{piece_registry::PieceRegistry, registry_error::RegistryResolveError},
    standard::tetromino_registry::standard_tetromino_registry,
};

pub fn resolve_piece_registry(
    profile: PieceSetProfile,
) -> Result<PieceRegistry, RegistryResolveError> {
    match profile.id() {
        PieceSetProfileId::StandardTetrominoes => Ok(standard_tetromino_registry()),
    }
}
