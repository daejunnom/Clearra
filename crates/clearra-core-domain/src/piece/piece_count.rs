#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PieceCount(u8);

impl PieceCount {
    pub fn new(value: u8) -> Result<Self, PieceCountError> {
        if value == 0 {
            return Err(PieceCountError::Zero);
        }
        Ok(Self(value))
    }
}
impl PieceCount {
    pub const fn new_unchecked(value: u8) -> Self {
        Self(value)
    }
}
impl PieceCount {
    pub fn get(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PieceCountError {
    Zero,
}
