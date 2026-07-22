#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SpinInterpretationScore {
    score: u64,
    attack: u32,
    probability_complete: bool,
}

impl SpinInterpretationScore {
    pub const fn new(score: u64, attack: u32, probability_complete: bool) -> Self {
        Self {
            score,
            attack,
            probability_complete,
        }
    }
}
impl SpinInterpretationScore {
    pub fn score(self) -> u64 {
        self.score
    }
}
impl SpinInterpretationScore {
    pub fn attack(self) -> u32 {
        self.attack
    }
}
impl SpinInterpretationScore {
    pub fn probability_complete(self) -> bool {
        self.probability_complete
    }
}
