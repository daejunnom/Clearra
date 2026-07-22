#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpinEvent {
    piece: char,
    mini: bool,
    lines: u8,
}

impl SpinEvent {
    pub fn new(piece: char, mini: bool, lines: u8) -> Self {
        Self {
            piece: piece.to_ascii_uppercase(),
            mini,
            lines,
        }
    }
}
impl SpinEvent {
    pub fn piece(self) -> char {
        self.piece
    }
}
impl SpinEvent {
    pub fn is_mini(self) -> bool {
        self.mini
    }
}
impl SpinEvent {
    pub fn lines(self) -> u8 {
        self.lines
    }
}
