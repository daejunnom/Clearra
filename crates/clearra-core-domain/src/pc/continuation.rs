use crate::ids::checkpoint_id::CheckpointId;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Continuation {
    from: CheckpointId,
    to: CheckpointId,
    lines: u8,
}

impl Continuation {
    pub fn new(from: CheckpointId, to: CheckpointId, lines: u8) -> Self {
        Self { from, to, lines }
    }
}
impl Continuation {
    pub fn from(self) -> CheckpointId {
        self.from
    }
}
impl Continuation {
    pub fn to(self) -> CheckpointId {
        self.to
    }
}
impl Continuation {
    pub fn lines(self) -> u8 {
        self.lines
    }
}
