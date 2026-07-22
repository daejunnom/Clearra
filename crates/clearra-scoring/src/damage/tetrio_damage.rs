use crate::event::SpinEvent;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TetrioDamageAction {
    NoClear,
    LineClear(u8),
    TSpin { mini: bool, lines: u8 },
    AllSpinMini { lines: u8 },
}

impl TetrioDamageAction {
    pub fn from_clear(lines: u8, spin: Option<SpinEvent>) -> Self {
        if lines == 0 {
            return Self::NoClear;
        }
        match spin {
            Some(spin) if spin.piece() == 'T' => Self::TSpin {
                mini: spin.is_mini(),
                lines,
            },
            Some(_) => Self::AllSpinMini { lines },
            None => Self::LineClear(lines),
        }
    }

    pub const fn lines(self) -> u8 {
        match self {
            Self::NoClear => 0,
            Self::LineClear(lines) | Self::TSpin { lines, .. } | Self::AllSpinMini { lines } => {
                lines
            }
        }
    }

    pub const fn is_difficult(self, perfect_clear: bool) -> bool {
        perfect_clear
            || (self.lines() > 0
                && matches!(
                    self,
                    Self::LineClear(4) | Self::TSpin { .. } | Self::AllSpinMini { .. }
                ))
    }

    const fn base_damage(self) -> u32 {
        match self {
            Self::NoClear | Self::LineClear(1) => 0,
            Self::LineClear(2) => 1,
            Self::LineClear(3) => 2,
            Self::LineClear(4) => 4,
            Self::LineClear(_) => 0,
            Self::TSpin {
                mini: false,
                lines: 1,
            } => 2,
            Self::TSpin {
                mini: false,
                lines: 2,
            } => 4,
            Self::TSpin {
                mini: false,
                lines: 3,
            } => 6,
            Self::TSpin {
                mini: true,
                lines: 2,
            } => 1,
            Self::TSpin { .. } => 0,
            Self::AllSpinMini { lines: 1 } => 0,
            Self::AllSpinMini { lines: 2 } => 1,
            Self::AllSpinMini { lines: 3 } => 2,
            Self::AllSpinMini { lines: 4 } => 3,
            Self::AllSpinMini { .. } => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct TetrioDamageState {
    combo: Option<u16>,
    back_to_back: Option<u16>,
    total_damage: u32,
}

impl TetrioDamageState {
    pub const fn new(combo: Option<u16>, back_to_back: Option<u16>) -> Self {
        Self {
            combo,
            back_to_back,
            total_damage: 0,
        }
    }

    pub const fn combo(self) -> Option<u16> {
        self.combo
    }

    pub const fn back_to_back(self) -> Option<u16> {
        self.back_to_back
    }

    pub const fn total_damage(self) -> u32 {
        self.total_damage
    }

    pub const fn from_parts(
        combo: Option<u16>,
        back_to_back: Option<u16>,
        total_damage: u32,
    ) -> Self {
        Self {
            combo,
            back_to_back,
            total_damage,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TetrioDamageEvaluation {
    damage: u32,
    state: TetrioDamageState,
    action: TetrioDamageAction,
    back_to_back_bonus: bool,
}

impl TetrioDamageEvaluation {
    pub const fn damage(self) -> u32 {
        self.damage
    }

    pub const fn state(self) -> TetrioDamageState {
        self.state
    }

    pub const fn action(self) -> TetrioDamageAction {
        self.action
    }

    pub const fn back_to_back_bonus(self) -> bool {
        self.back_to_back_bonus
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TetrioDamageProfile;

impl TetrioDamageProfile {
    pub const ID: &'static str = "tetrio-damage";

    pub fn evaluate(
        self,
        state: TetrioDamageState,
        action: TetrioDamageAction,
        perfect_clear: bool,
    ) -> TetrioDamageEvaluation {
        let action = if action.lines() == 0 {
            TetrioDamageAction::NoClear
        } else {
            action
        };
        let lines = action.lines();
        let combo = if lines == 0 {
            None
        } else {
            Some(state.combo.map_or(0, |combo| combo.saturating_add(1)))
        };
        let difficult = action.is_difficult(perfect_clear);
        let back_to_back_bonus = !perfect_clear && difficult && state.back_to_back.is_some();
        let back_to_back = if difficult {
            let increment = if perfect_clear { 2 } else { 1 };
            Some(
                state
                    .back_to_back
                    .map_or(increment - 1, |chain| chain.saturating_add(increment)),
            )
        } else if lines == 0 {
            state.back_to_back
        } else {
            None
        };

        let combo_index = u32::from(combo.unwrap_or(0));
        let base = action.base_damage();
        let mut damage = if lines == 0 {
            0
        } else if base > 0 {
            base.saturating_mul(4 + combo_index) / 4
        } else {
            ((1.0_f64 + 1.25_f64 * f64::from(combo_index)).ln().floor()) as u32
        };
        if perfect_clear {
            damage = 5;
        } else if back_to_back_bonus {
            damage = damage.saturating_add(1);
        }

        let next = TetrioDamageState {
            combo,
            back_to_back,
            total_damage: state.total_damage.saturating_add(damage),
        };
        TetrioDamageEvaluation {
            damage,
            state: next,
            action,
            back_to_back_bonus,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pensil_base_damage_table_is_preserved() {
        let profile = TetrioDamageProfile;
        let state = TetrioDamageState::default();
        let cases = [
            (TetrioDamageAction::LineClear(1), 0),
            (TetrioDamageAction::LineClear(2), 1),
            (TetrioDamageAction::LineClear(3), 2),
            (TetrioDamageAction::LineClear(4), 4),
            (
                TetrioDamageAction::TSpin {
                    mini: false,
                    lines: 1,
                },
                2,
            ),
            (
                TetrioDamageAction::TSpin {
                    mini: false,
                    lines: 2,
                },
                4,
            ),
            (
                TetrioDamageAction::TSpin {
                    mini: false,
                    lines: 3,
                },
                6,
            ),
            (
                TetrioDamageAction::TSpin {
                    mini: true,
                    lines: 2,
                },
                1,
            ),
            (TetrioDamageAction::AllSpinMini { lines: 4 }, 3),
        ];
        for (action, expected) in cases {
            assert_eq!(profile.evaluate(state, action, false).damage(), expected);
        }
    }

    #[test]
    fn combo_multiplier_and_zero_base_logarithm_match_calculator() {
        let profile = TetrioDamageProfile;
        let mut state = TetrioDamageState::default();
        for expected in [0, 0, 1, 1] {
            let evaluated = profile.evaluate(state, TetrioDamageAction::LineClear(1), false);
            assert_eq!(evaluated.damage(), expected);
            state = evaluated.state();
        }
        let evaluated = profile.evaluate(
            TetrioDamageState::new(Some(4), None),
            TetrioDamageAction::LineClear(4),
            false,
        );
        assert_eq!(evaluated.damage(), 9);
    }

    #[test]
    fn second_difficult_clear_receives_back_to_back_bonus() {
        let profile = TetrioDamageProfile;
        let first = profile.evaluate(
            TetrioDamageState::default(),
            TetrioDamageAction::LineClear(4),
            false,
        );
        assert!(!first.back_to_back_bonus());
        let second = profile.evaluate(first.state(), TetrioDamageAction::LineClear(4), false);
        assert!(second.back_to_back_bonus());
        assert_eq!(second.damage(), 6);
    }

    #[test]
    fn back_to_back_bonus_is_added_after_the_combo_multiplier() {
        let evaluated = TetrioDamageProfile.evaluate(
            TetrioDamageState::new(Some(3), Some(0)),
            TetrioDamageAction::TSpin {
                mini: false,
                lines: 1,
            },
            false,
        );

        assert_eq!(evaluated.state().combo(), Some(4));
        assert_eq!(evaluated.damage(), 5);
    }

    #[test]
    fn season_two_perfect_clear_adds_five_and_advances_b2b_by_two() {
        let evaluated = TetrioDamageProfile.evaluate(
            TetrioDamageState::default(),
            TetrioDamageAction::LineClear(2),
            true,
        );

        assert_eq!(evaluated.damage(), 5);
        assert_eq!(evaluated.state().back_to_back(), Some(1));

        let continued = TetrioDamageProfile.evaluate(
            TetrioDamageState::new(None, Some(2)),
            TetrioDamageAction::LineClear(2),
            true,
        );
        assert!(!continued.back_to_back_bonus());
        assert_eq!(continued.damage(), 5);
        assert_eq!(continued.state().back_to_back(), Some(4));
    }

    #[test]
    fn no_clear_resets_combo_but_preserves_back_to_back() {
        let profile = TetrioDamageProfile;
        let state = TetrioDamageState::new(Some(3), Some(2));
        let evaluated = profile.evaluate(state, TetrioDamageAction::NoClear, false);
        assert_eq!(evaluated.state().combo(), None);
        assert_eq!(evaluated.state().back_to_back(), Some(2));
    }

    #[test]
    fn zero_line_spin_resets_combo_and_does_not_advance_back_to_back() {
        let profile = TetrioDamageProfile;
        let action = TetrioDamageAction::from_clear(0, Some(SpinEvent::new('T', false, 0)));
        assert_eq!(action, TetrioDamageAction::NoClear);

        let fresh = profile.evaluate(TetrioDamageState::default(), action, false);
        assert_eq!(fresh.damage(), 0);
        assert_eq!(fresh.state().back_to_back(), None);

        let chained = profile.evaluate(TetrioDamageState::new(Some(3), Some(2)), action, false);
        assert_eq!(chained.state().combo(), None);
        assert_eq!(chained.state().back_to_back(), Some(2));

        let all_spin_action = TetrioDamageAction::from_clear(0, Some(SpinEvent::new('S', true, 0)));
        assert_eq!(all_spin_action, TetrioDamageAction::NoClear);
        let all_spin = profile.evaluate(
            TetrioDamageState::new(Some(4), Some(1)),
            all_spin_action,
            false,
        );
        assert_eq!(all_spin.state().combo(), None);
        assert_eq!(all_spin.state().back_to_back(), Some(1));
    }
}
