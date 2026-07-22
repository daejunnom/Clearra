use clearra_core_domain::ids::checkpoint_id::CheckpointId;

use super::continuation_token::ContinuationToken;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContinuationEdge {
    from: CheckpointId,
    to: CheckpointId,
    token: ContinuationToken,
}

impl ContinuationEdge {
    pub fn new(partition_index: u32, from: CheckpointId, to: CheckpointId) -> Self {
        Self {
            from,
            to,
            token: ContinuationToken::new(partition_index, from, to),
        }
    }
}
impl ContinuationEdge {
    pub fn from(self) -> CheckpointId {
        self.from
    }
}
impl ContinuationEdge {
    pub fn to(self) -> CheckpointId {
        self.to
    }
}
impl ContinuationEdge {
    pub fn token(self) -> ContinuationToken {
        self.token
    }
}
