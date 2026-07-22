#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PatternId(usize);

impl PatternId {
    pub const fn new(index: usize) -> Self {
        Self(index)
    }
}
impl PatternId {
    pub fn index(self) -> usize {
        self.0
    }
}
