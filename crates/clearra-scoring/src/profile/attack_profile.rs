use clearra_spin::SpinAwardClass;

use super::{AttackModelId, B2BPolicy, ComboPolicy};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineClearAttackTable {
    values: [u32; 5],
}

impl LineClearAttackTable {
    pub const fn guideline() -> Self {
        Self {
            values: [0, 0, 1, 2, 4],
        }
    }
}
impl LineClearAttackTable {
    pub fn attack(self, lines: u8) -> u32 {
        self.values[line_index(lines)]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpinAttackTable {
    mini: [u32; 5],
    regular: [u32; 5],
    all_spin: [u32; 5],
    all_mini: [u32; 5],
    special: [u32; 5],
}

impl SpinAttackTable {
    pub const fn guideline() -> Self {
        Self {
            mini: [0, 0, 1, 2, 4],
            regular: [0, 2, 4, 6, 8],
            all_spin: [0, 2, 4, 6, 8],
            all_mini: [0, 0, 1, 2, 4],
            special: [0, 2, 4, 6, 8],
        }
    }
}
impl SpinAttackTable {
    pub fn attack(self, award_class: SpinAwardClass, lines: u8) -> u32 {
        let values = match award_class {
            SpinAwardClass::None | SpinAwardClass::Unknown => return 0,
            SpinAwardClass::Mini => self.mini,
            SpinAwardClass::Regular => self.regular,
            SpinAwardClass::AllSpin => self.all_spin,
            SpinAwardClass::AllMini => self.all_mini,
            SpinAwardClass::Special => self.special,
        };
        values[line_index(lines)]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AllClearAttackPolicy {
    Disabled,
    Flat(u32),
}

impl AllClearAttackPolicy {
    pub fn attack(self, perfect_clear: bool) -> u32 {
        match (self, perfect_clear) {
            (Self::Flat(value), true) => value,
            _ => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComboAttackPolicy {
    enabled: bool,
    attack_bonus_per_combo: u32,
}

impl ComboAttackPolicy {
    pub const DISABLED: Self = Self {
        enabled: false,
        attack_bonus_per_combo: 0,
    };
}
impl ComboAttackPolicy {
    pub const fn linear(attack_bonus_per_combo: u32) -> Self {
        Self {
            enabled: true,
            attack_bonus_per_combo,
        }
    }
}
impl ComboAttackPolicy {
    pub fn from_combo_policy(policy: ComboPolicy) -> Self {
        if policy.enabled() {
            Self::linear(policy.attack_bonus_per_combo())
        } else {
            Self::DISABLED
        }
    }
}
impl ComboAttackPolicy {
    pub fn attack_bonus(self, combo_after: u8) -> u32 {
        if !self.enabled {
            return 0;
        }
        u32::from(combo_after.saturating_sub(1)).saturating_mul(self.attack_bonus_per_combo)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct B2bAttackPolicy {
    enabled: bool,
    attack_bonus: u32,
}

impl B2bAttackPolicy {
    pub const DISABLED: Self = Self {
        enabled: false,
        attack_bonus: 0,
    };
}
impl B2bAttackPolicy {
    pub const fn standard(attack_bonus: u32) -> Self {
        Self {
            enabled: true,
            attack_bonus,
        }
    }
}
impl B2bAttackPolicy {
    pub fn from_b2b_policy(policy: B2BPolicy) -> Self {
        if policy.enabled() {
            Self::standard(policy.attack_bonus())
        } else {
            Self::DISABLED
        }
    }
}
impl B2bAttackPolicy {
    pub fn attack_bonus(self, difficult_clear: bool, b2b_before: bool) -> u32 {
        if self.enabled && difficult_clear && b2b_before {
            self.attack_bonus
        } else {
            0
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AttackRoundingPolicy {
    #[default]
    Integer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttackProfile {
    attack_model: AttackModelId,
    line_clear_attack_table: LineClearAttackTable,
    spin_attack_table: SpinAttackTable,
    all_clear_attack: AllClearAttackPolicy,
    combo_attack_policy: ComboAttackPolicy,
    b2b_attack_policy: B2bAttackPolicy,
    rounding_policy: AttackRoundingPolicy,
}

impl AttackProfile {
    pub const fn disabled() -> Self {
        Self {
            attack_model: AttackModelId::Disabled,
            line_clear_attack_table: LineClearAttackTable { values: [0; 5] },
            spin_attack_table: SpinAttackTable {
                mini: [0; 5],
                regular: [0; 5],
                all_spin: [0; 5],
                all_mini: [0; 5],
                special: [0; 5],
            },
            all_clear_attack: AllClearAttackPolicy::Disabled,
            combo_attack_policy: ComboAttackPolicy::DISABLED,
            b2b_attack_policy: B2bAttackPolicy::DISABLED,
            rounding_policy: AttackRoundingPolicy::Integer,
        }
    }
}
impl AttackProfile {
    pub const fn guideline() -> Self {
        Self {
            attack_model: AttackModelId::Guideline,
            line_clear_attack_table: LineClearAttackTable::guideline(),
            spin_attack_table: SpinAttackTable::guideline(),
            all_clear_attack: AllClearAttackPolicy::Flat(10),
            combo_attack_policy: ComboAttackPolicy::DISABLED,
            b2b_attack_policy: B2bAttackPolicy::DISABLED,
            rounding_policy: AttackRoundingPolicy::Integer,
        }
    }
}
impl AttackProfile {
    pub fn from_policies(
        attack_model: AttackModelId,
        combo_policy: ComboPolicy,
        b2b_policy: B2BPolicy,
    ) -> Self {
        if attack_model == AttackModelId::Disabled {
            return Self::disabled();
        }
        Self {
            attack_model,
            combo_attack_policy: ComboAttackPolicy::from_combo_policy(combo_policy),
            b2b_attack_policy: B2bAttackPolicy::from_b2b_policy(b2b_policy),
            ..Self::guideline()
        }
    }
}
impl AttackProfile {
    pub fn attack_model(self) -> AttackModelId {
        self.attack_model
    }
}
impl AttackProfile {
    pub fn attack_for_line_clear(self, lines: u8, perfect_clear: bool) -> u32 {
        if self.attack_model == AttackModelId::Disabled {
            return 0;
        }
        self.line_clear_attack_table
            .attack(lines)
            .saturating_add(self.all_clear_attack.attack(perfect_clear))
    }
}
impl AttackProfile {
    pub fn attack_for_award(
        self,
        award_class: SpinAwardClass,
        lines: u8,
        perfect_clear: bool,
    ) -> u32 {
        if self.attack_model == AttackModelId::Disabled {
            return 0;
        }
        let spin_attack = self.spin_attack_table.attack(award_class, lines);
        let line_attack = if award_class == SpinAwardClass::None {
            self.line_clear_attack_table.attack(lines)
        } else {
            spin_attack
        };
        line_attack.saturating_add(self.all_clear_attack.attack(perfect_clear))
    }
}
impl AttackProfile {
    pub fn combo_attack_bonus(self, combo_after: u8) -> u32 {
        self.combo_attack_policy.attack_bonus(combo_after)
    }
}
impl AttackProfile {
    pub fn b2b_attack_bonus(self, difficult_clear: bool, b2b_before: bool) -> u32 {
        self.b2b_attack_policy
            .attack_bonus(difficult_clear, b2b_before)
    }
}
impl AttackProfile {
    pub fn rounding_policy(self) -> AttackRoundingPolicy {
        self.rounding_policy
    }
}

fn line_index(lines: u8) -> usize {
    usize::from(lines.min(4))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attack_profile_separate_from_score_profile() {
        let attack_profile = AttackProfile::guideline();

        assert_eq!(attack_profile.attack_model(), AttackModelId::Guideline);
        assert_eq!(attack_profile.attack_for_line_clear(4, true), 14);
        assert_eq!(
            attack_profile.attack_for_award(SpinAwardClass::Regular, 2, false),
            4
        );
    }
}
