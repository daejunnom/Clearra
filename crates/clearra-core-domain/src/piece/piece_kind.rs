#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PieceKind {
    I,
    O,
    T,
    S,
    Z,
    J,
    L,
}

impl PieceKind {
    pub const STANDARD_TETROMINOES: [Self; 7] = [
        Self::I,
        Self::O,
        Self::T,
        Self::S,
        Self::Z,
        Self::J,
        Self::L,
    ];
}
impl PieceKind {
    pub fn from_ascii(value: char) -> Result<Self, UnknownPieceKind> {
        match value.to_ascii_uppercase() {
            'I' => Ok(Self::I),
            'O' => Ok(Self::O),
            'T' => Ok(Self::T),
            'S' => Ok(Self::S),
            'Z' => Ok(Self::Z),
            'J' => Ok(Self::J),
            'L' => Ok(Self::L),
            _ => Err(UnknownPieceKind),
        }
    }
}
impl PieceKind {
    pub fn as_ascii(self) -> char {
        match self {
            Self::I => 'I',
            Self::O => 'O',
            Self::T => 'T',
            Self::S => 'S',
            Self::Z => 'Z',
            Self::J => 'J',
            Self::L => 'L',
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnknownPieceKind;

#[cfg(test)]
#[path = "piece_kind_tests.rs"]
mod tests;
