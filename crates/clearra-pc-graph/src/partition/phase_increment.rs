#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PhaseIncrement {
    lines: u8,
}

impl PhaseIncrement {
    pub fn new(lines: u8) -> Result<Self, PhaseIncrementError> {
        // Empty-to-empty phases have the same area invariant as a PcTarget.
        clearra_core_domain::pc::pc_target::PcTarget::new(lines)
            .map(|_| Self { lines })
            .map_err(|_| PhaseIncrementError::UnsupportedLineCount { lines })
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
