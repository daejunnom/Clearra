use clearra_core_domain::ids::checkpoint_id::CheckpointId;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ContinuationToken(u64);

impl ContinuationToken {
    pub fn new(partition_index: u32, from: CheckpointId, to: CheckpointId) -> Self {
        let value = (u64::from(partition_index) << 48)
            | (u64::from(from.get()) << 24)
            | u64::from(to.get());
        Self(value)
    }
}
impl ContinuationToken {
    pub fn get(self) -> u64 {
        self.0
    }
}
