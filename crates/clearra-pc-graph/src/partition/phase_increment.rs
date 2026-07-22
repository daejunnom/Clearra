#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PhaseIncrement {
    lines: u8,
}

impl PhaseIncrement {
    pub fn new(lines: u8) -> Result<Self, PhaseIncrementError> {
        match lines {
            2 | 4 | 6 => Ok(Self { lines }),
            _ => Err(PhaseIncrementError::UnsupportedLineCount { lines }),
        }
    }
}
impl PhaseIncrement {
    pub fn lines(self) -> u8 {
        self.lines
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PhaseIncrementError {
    UnsupportedLineCount { lines: u8 },
}
