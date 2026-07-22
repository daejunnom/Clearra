#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CheckpointId(u32);

impl CheckpointId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}
impl CheckpointId {
    pub fn get(self) -> u32 {
        self.0
    }
}
