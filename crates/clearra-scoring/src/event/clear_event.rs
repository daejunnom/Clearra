#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ClearEvent {
    lines: u8,
    perfect_clear: bool,
}

impl ClearEvent {
    pub fn new(lines: u8, perfect_clear: bool) -> Self {
        Self {
            lines,
            perfect_clear,
        }
    }
}
impl ClearEvent {
    pub fn lines(self) -> u8 {
        self.lines
    }
}
impl ClearEvent {
    pub fn is_perfect_clear(self) -> bool {
        self.perfect_clear
    }
}
