#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PatternUniverseId(u64);

impl PatternUniverseId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}
impl PatternUniverseId {
    pub const fn get(self) -> u64 {
        self.0
    }
}
