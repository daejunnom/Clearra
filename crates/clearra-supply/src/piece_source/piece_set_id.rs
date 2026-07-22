#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PieceSetId(u8);

impl PieceSetId {
    pub const STANDARD_TETROMINOES: Self = Self(1);
}
impl PieceSetId {
    pub const fn new(value: u8) -> Self {
        Self(value)
    }
}
impl PieceSetId {
    pub const fn get(self) -> u8 {
        self.0
    }
}
