use crate::codec::{FumenLikeReadError, FumenLikeReader, FumenLikeTrace};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FumenToReplayAdapter;

impl FumenToReplayAdapter {
    pub fn read_trace(input: &str) -> Result<FumenLikeTrace, FumenToReplayError> {
        FumenLikeReader::read(input).map_err(FumenToReplayError::Read)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FumenToReplayError {
    Read(FumenLikeReadError),
    ReplayAdapterUnavailable,
}
