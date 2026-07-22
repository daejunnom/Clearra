#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RotationState {
    #[default]
    Zero,
    Right,
    Two,
    Left,
}

impl RotationState {
    pub const ALL: [Self; 4] = [Self::Zero, Self::Right, Self::Two, Self::Left];
}
impl RotationState {
    pub fn from_quarter_turns(turns: u8) -> Result<Self, InvalidRotationQuarterTurns> {
        match turns {
            0 => Ok(Self::Zero),
            1 => Ok(Self::Right),
            2 => Ok(Self::Two),
            3 => Ok(Self::Left),
            _ => Err(InvalidRotationQuarterTurns),
        }
    }
}
impl RotationState {
    pub fn quarter_turns(self) -> u8 {
        match self {
            Self::Zero => 0,
            Self::Right => 1,
            Self::Two => 2,
            Self::Left => 3,
        }
    }
}
impl RotationState {
    pub fn clockwise(self) -> Self {
        match self {
            Self::Zero => Self::Right,
            Self::Right => Self::Two,
            Self::Two => Self::Left,
            Self::Left => Self::Zero,
        }
    }
}
impl RotationState {
    pub fn counter_clockwise(self) -> Self {
        match self {
            Self::Zero => Self::Left,
            Self::Right => Self::Zero,
            Self::Two => Self::Right,
            Self::Left => Self::Two,
        }
    }
}
impl RotationState {
    pub fn rotated_180(self) -> Self {
        self.clockwise().clockwise()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidRotationQuarterTurns;

#[cfg(test)]
#[path = "rotation_tests.rs"]
mod tests;
