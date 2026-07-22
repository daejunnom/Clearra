#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineClearEvent {
    cleared_lines: u8,
}

impl LineClearEvent {
    pub fn new(cleared_lines: u8) -> Self {
        Self { cleared_lines }
    }
}
impl LineClearEvent {
    pub fn cleared_lines(self) -> u8 {
        self.cleared_lines
    }
}
