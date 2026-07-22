#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayStepView {
    step_index: usize,
    piece: char,
    rotation: u8,
    x: i32,
    y: i32,
    hold: String,
    cleared_lines: u8,
}

impl ReplayStepView {
    pub fn new(
        step_index: usize,
        piece: char,
        rotation: u8,
        x: i32,
        y: i32,
        hold: impl Into<String>,
        cleared_lines: u8,
    ) -> Self {
        Self {
            step_index,
            piece,
            rotation,
            x,
            y,
            hold: hold.into(),
            cleared_lines,
        }
    }
}
impl ReplayStepView {
    pub const fn step_index(&self) -> usize {
        self.step_index
    }
}
impl ReplayStepView {
    pub const fn piece(&self) -> char {
        self.piece
    }
}
impl ReplayStepView {
    pub const fn rotation(&self) -> u8 {
        self.rotation
    }
}
impl ReplayStepView {
    pub const fn x(&self) -> i32 {
        self.x
    }
}
impl ReplayStepView {
    pub const fn y(&self) -> i32 {
        self.y
    }
}
impl ReplayStepView {
    pub fn hold(&self) -> &str {
        &self.hold
    }
}
impl ReplayStepView {
    pub const fn cleared_lines(&self) -> u8 {
        self.cleared_lines
    }
}
