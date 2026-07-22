use crate::{ids::checkpoint_id::CheckpointId, pc::pc_target::PcTarget};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Checkpoint {
    id: CheckpointId,
    cleared_lines: u8,
    target: PcTarget,
}

impl Checkpoint {
    pub fn new(id: CheckpointId, cleared_lines: u8, target: PcTarget) -> Self {
        Self {
            id,
            cleared_lines,
            target,
        }
    }
}
impl Checkpoint {
    pub fn id(self) -> CheckpointId {
        self.id
    }
}
impl Checkpoint {
    pub fn cleared_lines(self) -> u8 {
        self.cleared_lines
    }
}
impl Checkpoint {
    pub fn target(self) -> PcTarget {
        self.target
    }
}
impl Checkpoint {
    pub fn is_terminal(self) -> bool {
        self.cleared_lines >= self.target.lines()
    }
}
