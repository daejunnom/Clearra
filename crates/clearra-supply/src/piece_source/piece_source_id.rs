#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PieceSourceId(u64);

impl PieceSourceId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
}
impl PieceSourceId {
    pub const fn get(self) -> u64 {
        self.0
    }
}
