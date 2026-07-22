#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayLineClearView {
    step_index: usize,
    cleared_lines: u8,
    has_line_clear: bool,
}

impl ReplayLineClearView {
    pub const fn new(step_index: usize, cleared_lines: u8) -> Self {
        Self {
            step_index,
            cleared_lines,
            has_line_clear: cleared_lines > 0,
        }
    }
}
impl ReplayLineClearView {
    pub const fn step_index(&self) -> usize {
        self.step_index
    }
}
impl ReplayLineClearView {
    pub const fn cleared_lines(&self) -> u8 {
        self.cleared_lines
    }
}
impl ReplayLineClearView {
    pub const fn has_line_clear(&self) -> bool {
        self.has_line_clear
    }
}
