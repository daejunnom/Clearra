#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct GuiJobId(u64);

impl GuiJobId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}
impl GuiJobId {
    pub const fn get(self) -> u64 {
        self.0
    }
}
