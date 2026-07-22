use clearra_core_domain::ids::checkpoint_id::CheckpointId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CheckpointNode {
    id: CheckpointId,
    partition_index: u32,
    phase_index: u32,
    lines_to_clear: u8,
    cumulative_lines: u8,
}

impl CheckpointNode {
    pub fn new(
        id: CheckpointId,
        partition_index: u32,
        phase_index: u32,
        lines_to_clear: u8,
        cumulative_lines: u8,
    ) -> Self {
        Self {
            id,
            partition_index,
            phase_index,
            lines_to_clear,
            cumulative_lines,
        }
    }
}
impl CheckpointNode {
    pub fn id(self) -> CheckpointId {
        self.id
    }
}
impl CheckpointNode {
    pub fn partition_index(self) -> u32 {
        self.partition_index
    }
}
impl CheckpointNode {
    pub fn phase_index(self) -> u32 {
        self.phase_index
    }
}
impl CheckpointNode {
    pub fn lines_to_clear(self) -> u8 {
        self.lines_to_clear
    }
}
impl CheckpointNode {
    pub fn cumulative_lines(self) -> u8 {
        self.cumulative_lines
    }
}
