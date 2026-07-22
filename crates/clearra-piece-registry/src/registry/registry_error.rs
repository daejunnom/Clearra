use clearra_profiles::pieces::piece_set_profile::PieceSetProfileId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistryResolveError {
    UnsupportedPieceSet(PieceSetProfileId),
}
