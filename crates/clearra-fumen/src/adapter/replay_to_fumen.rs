use crate::codec::{FumenLikeTrace, FumenLikeWriteError, FumenLikeWriter};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ReplayToFumenAdapter;

impl ReplayToFumenAdapter {
    pub fn trace_to_fumen(
        trace: &clearra_replay::ReplayTrace,
    ) -> Result<String, FumenLikeWriteError> {
        FumenLikeWriter::write_replay_trace(trace)
    }
}
impl ReplayToFumenAdapter {
    pub fn pages_to_fumen(pages: Vec<String>) -> Result<String, FumenLikeWriteError> {
        FumenLikeWriter::write(&FumenLikeTrace::new(pages))
    }
}
