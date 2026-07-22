use super::combo_state::ComboState;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScoreState {
    score: u64,
    attack: u32,
    combo: ComboState,
    b2b_chain: u32,
}

impl ScoreState {
    pub fn new(score: u64, attack: u32) -> Self {
        Self {
            score,
            attack,
            combo: ComboState::default(),
            b2b_chain: 0,
        }
    }
}
impl ScoreState {
    pub fn score(self) -> u64 {
        self.score
    }
}
impl ScoreState {
    pub fn attack(self) -> u32 {
        self.attack
    }
}
impl ScoreState {
    pub fn combo(self) -> ComboState {
        self.combo
    }
}
impl ScoreState {
    pub fn b2b_active(self) -> bool {
        self.b2b_chain > 0
    }
}
impl ScoreState {
    pub fn b2b_chain(self) -> u32 {
        self.b2b_chain
    }
}
impl ScoreState {
    pub fn add_score(self, value: u64) -> Self {
        Self {
            score: self.score.saturating_add(value),
            ..self
        }
    }
}
impl ScoreState {
    pub fn add_attack(self, value: u32) -> Self {
        Self {
            attack: self.attack.saturating_add(value),
            ..self
        }
    }
}
impl ScoreState {
    pub fn with_combo(self, combo: ComboState) -> Self {
        Self { combo, ..self }
    }
}
impl ScoreState {
    pub fn with_b2b_active(self, b2b_active: bool) -> Self {
        Self {
            b2b_chain: u32::from(b2b_active),
            ..self
        }
    }
}
impl ScoreState {
    pub fn with_b2b_chain(self, b2b_chain: u32) -> Self {
        Self { b2b_chain, ..self }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_state_tracks_score_attack_combo_and_b2b_without_search_state() {
        let state = ScoreState::new(10, 2)
            .with_combo(ComboState::new(3))
            .with_b2b_active(true)
            .add_score(5)
            .add_attack(4);

        assert_eq!(state.score(), 15);
        assert_eq!(state.attack(), 6);
        assert_eq!(state.combo().combo(), 3);
        assert!(state.b2b_active());
    }
}
