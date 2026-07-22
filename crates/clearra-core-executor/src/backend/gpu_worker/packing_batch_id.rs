#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackingBatchId(u64);

impl PackingBatchId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}
impl PackingBatchId {
    pub const fn get(self) -> u64 {
        self.0
    }
}
