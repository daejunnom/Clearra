#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PlacementId(u32);

impl PlacementId {
    pub const fn new(value: u32) -> Self {
        Self(value)
    }
}
impl PlacementId {
    pub fn get(self) -> u32 {
        self.0
    }
}
