#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PatternWeightModelId(u64);

impl PatternWeightModelId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}
impl PatternWeightModelId {
    pub const fn get(self) -> u64 {
        self.0
    }
}
