#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PcTarget {
    lines: u8,
}

impl PcTarget {
    pub fn new(lines: u8) -> Result<Self, PcTargetError> {
        if lines == 0 {
            return Err(PcTargetError::ZeroLines);
        }
        if !lines.is_multiple_of(2) {
            return Err(PcTargetError::OddLineCount { lines });
        }
        if lines > STANDARD_PC_MAX_LINES {
            return Err(PcTargetError::TooManyLines {
                lines,
                maximum: STANDARD_PC_MAX_LINES,
            });
        }
        Ok(Self { lines })
    }
}
impl PcTarget {
    pub const fn two_lines() -> Self {
        Self { lines: 2 }
    }
}
impl PcTarget {
    pub const fn four_lines() -> Self {
        Self { lines: 4 }
    }
}
impl PcTarget {
    pub const fn six_lines() -> Self {
        Self { lines: 6 }
    }
}
impl PcTarget {
    pub fn lines(self) -> u8 {
        self.lines
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcTargetError {
    ZeroLines,
    OddLineCount { lines: u8 },
    TooManyLines { lines: u8, maximum: u8 },
}

#[cfg(test)]
#[path = "pc_target_tests.rs"]
mod tests;
use crate::board::standard_pc_board::STANDARD_PC_MAX_LINES;
