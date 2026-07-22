mod attack_model_id {
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub enum AttackModelId {
        #[default]
        Disabled,
        Guideline,
        Ppt,
        Tetrio,
    }

    impl AttackModelId {
        pub fn parse(value: &str) -> Option<Self> {
            match value.to_ascii_lowercase().as_str() {
                "disabled" | "none" => Some(Self::Disabled),
                "guideline" => Some(Self::Guideline),
                "ppt" | "puyo-puyo-tetris" => Some(Self::Ppt),
                "tetrio" => Some(Self::Tetrio),
                _ => None,
            }
        }
    }
    impl AttackModelId {
        pub fn as_str(self) -> &'static str {
            match self {
                Self::Disabled => "disabled",
                Self::Guideline => "guideline",
                Self::Ppt => "ppt",
                Self::Tetrio => "tetrio",
            }
        }
    }
}
mod b2b_policy {
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub enum B2BChainRule {
        #[default]
        UnderlyingDifficultClearOnly,
    }

    impl B2BChainRule {
        pub const fn as_str(self) -> &'static str {
            match self {
                Self::UnderlyingDifficultClearOnly => "underlying-difficult-clear-only",
            }
        }

        pub const fn all_clear_extra_increment(self) -> u32 {
            match self {
                Self::UnderlyingDifficultClearOnly => 0,
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum B2BScoreRule {
        FlatBonus(u64),
        Multiplier { numerator: u32, denominator: u32 },
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct B2BPolicy {
        enabled: bool,
        score_rule: B2BScoreRule,
        attack_bonus: u32,
        chain_rule: B2BChainRule,
    }

    impl B2BPolicy {
        pub const DISABLED: Self = Self {
            enabled: false,
            score_rule: B2BScoreRule::FlatBonus(0),
            attack_bonus: 0,
            chain_rule: B2BChainRule::UnderlyingDifficultClearOnly,
        };
    }
    impl B2BPolicy {
        pub const fn standard(score_bonus: u64, attack_bonus: u32) -> Self {
            Self {
                enabled: true,
                score_rule: B2BScoreRule::FlatBonus(score_bonus),
                attack_bonus,
                chain_rule: B2BChainRule::UnderlyingDifficultClearOnly,
            }
        }

        pub const fn multiplier(numerator: u32, denominator: u32, attack_bonus: u32) -> Self {
            Self {
                enabled: true,
                score_rule: B2BScoreRule::Multiplier {
                    numerator,
                    denominator,
                },
                attack_bonus,
                chain_rule: B2BChainRule::UnderlyingDifficultClearOnly,
            }
        }
    }
    impl B2BPolicy {
        pub fn enabled(self) -> bool {
            self.enabled
        }
    }
    impl B2BPolicy {
        pub fn score_bonus(self) -> u64 {
            match self.score_rule {
                B2BScoreRule::FlatBonus(value) => value,
                B2BScoreRule::Multiplier { .. } => 0,
            }
        }
    }
    impl B2BPolicy {
        pub fn score_rule(self) -> B2BScoreRule {
            self.score_rule
        }
    }
    impl B2BPolicy {
        pub fn adjusted_score(self, base_score: u64) -> u64 {
            if !self.enabled {
                return base_score;
            }
            match self.score_rule {
                B2BScoreRule::FlatBonus(value) => base_score.saturating_add(value),
                B2BScoreRule::Multiplier {
                    numerator,
                    denominator,
                } => {
                    base_score.saturating_mul(u64::from(numerator)) / u64::from(denominator.max(1))
                }
            }
        }
    }
    impl B2BPolicy {
        pub fn attack_bonus(self) -> u32 {
            self.attack_bonus
        }
    }
    impl B2BPolicy {
        pub const fn chain_rule(self) -> B2BChainRule {
            self.chain_rule
        }
    }

    impl Default for B2BPolicy {
        fn default() -> Self {
            Self::DISABLED
        }
    }
}
mod combo_policy {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct ComboPolicy {
        enabled: bool,
        score_bonus_per_combo: u64,
        attack_bonus_per_combo: u32,
    }

    impl ComboPolicy {
        pub const DISABLED: Self = Self {
            enabled: false,
            score_bonus_per_combo: 0,
            attack_bonus_per_combo: 0,
        };
    }
    impl ComboPolicy {
        pub const fn linear(score_bonus_per_combo: u64, attack_bonus_per_combo: u32) -> Self {
            Self {
                enabled: true,
                score_bonus_per_combo,
                attack_bonus_per_combo,
            }
        }
    }
    impl ComboPolicy {
        pub fn enabled(self) -> bool {
            self.enabled
        }
    }
    impl ComboPolicy {
        pub fn score_bonus_per_combo(self) -> u64 {
            self.score_bonus_per_combo
        }
    }
    impl ComboPolicy {
        pub fn attack_bonus_per_combo(self) -> u32 {
            self.attack_bonus_per_combo
        }
    }

    impl Default for ComboPolicy {
        fn default() -> Self {
            Self::DISABLED
        }
    }
}
mod score_model_id {
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub enum ScoreModelId {
        #[default]
        Disabled,
        Guideline,
        JstrisUltra,
        Tetrio,
    }

    impl ScoreModelId {
        pub fn parse(value: &str) -> Option<Self> {
            match value.to_ascii_lowercase().as_str() {
                "disabled" | "none" => Some(Self::Disabled),
                "guideline" => Some(Self::Guideline),
                "jstris-ultra" => Some(Self::JstrisUltra),
                "tetrio" | "tetrio-score" => Some(Self::Tetrio),
                _ => None,
            }
        }
    }
    impl ScoreModelId {
        pub fn as_str(self) -> &'static str {
            match self {
                Self::Disabled => "disabled",
                Self::Guideline => "guideline",
                Self::JstrisUltra => "jstris-ultra",
                Self::Tetrio => "tetrio",
            }
        }
    }
}
mod score_profile_model {
    use super::{AttackModelId, B2BPolicy, ComboPolicy, ScoreModelId, ScoringAccuracyLevel};
    use crate::profile::{
        AllSpinScoreMapping, DropScorePolicy, LevelPolicy, PcBonusPolicy, SpinAwardPolicy,
        SpinProfile, SpinRuleId, TraceRequirement,
    };

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct ScoreProfile {
        id: String,
        display_name: String,
        score_model: ScoreModelId,
        attack_model: AttackModelId,
        spin_profile: SpinProfile,
        accuracy_level: ScoringAccuracyLevel,
        accuracy_reason: &'static str,
        combo_policy: ComboPolicy,
        b2b_policy: B2BPolicy,
        drop_score_policy: DropScorePolicy,
        level_policy: LevelPolicy,
        pc_bonus_policy: PcBonusPolicy,
        trace_requirement: TraceRequirement,
    }

    pub const BASIC_APPROXIMATION_REASON: &str =
        "profile-specific basic score/attack tables with configurable spin detection";

    impl ScoreProfile {
        pub fn new(id: impl Into<String>, display_name: impl Into<String>) -> Self {
            Self {
                id: id.into(),
                display_name: display_name.into(),
                score_model: ScoreModelId::Disabled,
                attack_model: AttackModelId::Disabled,
                spin_profile: SpinProfile::default(),
                accuracy_level: ScoringAccuracyLevel::BasicApproximation,
                accuracy_reason: BASIC_APPROXIMATION_REASON,
                combo_policy: ComboPolicy::DISABLED,
                b2b_policy: B2BPolicy::DISABLED,
                drop_score_policy: DropScorePolicy::Disabled,
                level_policy: LevelPolicy::Disabled,
                pc_bonus_policy: PcBonusPolicy::Disabled,
                trace_requirement: TraceRequirement::None,
            }
        }
    }
    impl ScoreProfile {
        pub fn with_score_enabled(mut self) -> Self {
            self.score_model = ScoreModelId::Guideline;
            self
        }
    }
    impl ScoreProfile {
        pub fn with_attack_enabled(mut self) -> Self {
            self.attack_model = AttackModelId::Guideline;
            self
        }
    }
    impl ScoreProfile {
        pub fn with_score_model(mut self, score_model: ScoreModelId) -> Self {
            self.score_model = score_model;
            self
        }
    }
    impl ScoreProfile {
        pub fn with_attack_model(mut self, attack_model: AttackModelId) -> Self {
            self.attack_model = attack_model;
            self
        }
    }
    impl ScoreProfile {
        pub fn with_spin_rule(mut self, spin_rule: SpinRuleId) -> Self {
            self.spin_profile = SpinProfile::builtin(spin_rule);
            self
        }
    }
    impl ScoreProfile {
        pub fn with_spin_profile(mut self, spin_profile: SpinProfile) -> Self {
            self.spin_profile = spin_profile;
            self
        }
    }
    impl ScoreProfile {
        pub fn with_accuracy(
            mut self,
            accuracy_level: ScoringAccuracyLevel,
            accuracy_reason: &'static str,
        ) -> Self {
            self.accuracy_level = accuracy_level;
            self.accuracy_reason = accuracy_reason;
            self
        }
    }
    impl ScoreProfile {
        pub fn with_combo_policy(mut self, combo_policy: ComboPolicy) -> Self {
            self.combo_policy = combo_policy;
            self
        }
    }
    impl ScoreProfile {
        pub fn with_b2b_policy(mut self, b2b_policy: B2BPolicy) -> Self {
            self.b2b_policy = b2b_policy;
            self
        }
    }
    impl ScoreProfile {
        pub fn with_spin_award_policy(mut self, spin_award_policy: SpinAwardPolicy) -> Self {
            self.spin_profile = self.spin_profile.with_award_policy(spin_award_policy);
            self
        }
    }
    impl ScoreProfile {
        pub fn with_all_spin_score_mapping(
            mut self,
            all_spin_score_mapping: AllSpinScoreMapping,
        ) -> Self {
            self.spin_profile = self
                .spin_profile
                .with_all_spin_score_mapping(all_spin_score_mapping);
            self
        }
    }
    impl ScoreProfile {
        pub fn with_drop_score_policy(mut self, drop_score_policy: DropScorePolicy) -> Self {
            self.drop_score_policy = drop_score_policy;
            if drop_score_policy.requires_drop_events() {
                self.trace_requirement = TraceRequirement::FullDropTrace;
            }
            self
        }
    }
    impl ScoreProfile {
        pub fn with_level_policy(mut self, level_policy: LevelPolicy) -> Self {
            self.level_policy = level_policy;
            self
        }
    }
    impl ScoreProfile {
        pub fn with_pc_bonus_policy(mut self, pc_bonus_policy: PcBonusPolicy) -> Self {
            self.pc_bonus_policy = pc_bonus_policy;
            self
        }
    }
    impl ScoreProfile {
        pub fn with_trace_requirement(mut self, trace_requirement: TraceRequirement) -> Self {
            self.trace_requirement = trace_requirement;
            self
        }
    }
    impl ScoreProfile {
        pub fn id(&self) -> &str {
            &self.id
        }
    }
    impl ScoreProfile {
        pub fn display_name(&self) -> &str {
            &self.display_name
        }
    }
    impl ScoreProfile {
        pub fn score_enabled(&self) -> bool {
            self.score_model != ScoreModelId::Disabled
        }
    }
    impl ScoreProfile {
        pub fn attack_enabled(&self) -> bool {
            self.attack_model != AttackModelId::Disabled
        }
    }
    impl ScoreProfile {
        pub fn score_model(&self) -> ScoreModelId {
            self.score_model
        }
    }
    impl ScoreProfile {
        pub fn score_model_id(&self) -> ScoreModelId {
            self.score_model
        }
    }
    impl ScoreProfile {
        pub fn attack_model(&self) -> AttackModelId {
            self.attack_model
        }
    }
    impl ScoreProfile {
        pub fn attack_model_id(&self) -> AttackModelId {
            self.attack_model
        }
    }
    impl ScoreProfile {
        pub fn spin_rule(&self) -> SpinRuleId {
            self.spin_profile.id()
        }
    }
    impl ScoreProfile {
        pub fn spin_rule_id(&self) -> SpinRuleId {
            self.spin_profile.id()
        }
    }
    impl ScoreProfile {
        pub fn spin_profile(&self) -> SpinProfile {
            self.spin_profile
        }
    }
    impl ScoreProfile {
        pub fn accuracy_level(&self) -> ScoringAccuracyLevel {
            self.accuracy_level
        }
    }
    impl ScoreProfile {
        pub fn accuracy_reason(&self) -> &'static str {
            self.accuracy_reason
        }
    }
    impl ScoreProfile {
        pub fn profile_specific_exact(&self) -> bool {
            self.accuracy_level == ScoringAccuracyLevel::ProfileSpecificExact
        }
    }
    impl ScoreProfile {
        pub fn combo_policy(&self) -> ComboPolicy {
            self.combo_policy
        }
    }
    impl ScoreProfile {
        pub fn b2b_policy(&self) -> B2BPolicy {
            self.b2b_policy
        }
    }
    impl ScoreProfile {
        pub fn spin_award_policy(&self) -> SpinAwardPolicy {
            self.spin_profile.award_policy()
        }
    }
    impl ScoreProfile {
        pub fn all_spin_score_mapping(&self) -> AllSpinScoreMapping {
            self.spin_profile.all_spin_score_mapping()
        }
    }
    impl ScoreProfile {
        pub fn drop_score_policy(&self) -> DropScorePolicy {
            self.drop_score_policy
        }
    }
    impl ScoreProfile {
        pub fn level_policy(&self) -> LevelPolicy {
            self.level_policy
        }
    }
    impl ScoreProfile {
        pub fn pc_bonus_policy(&self) -> PcBonusPolicy {
            self.pc_bonus_policy
        }
    }
    impl ScoreProfile {
        pub fn trace_requirement(&self) -> TraceRequirement {
            self.trace_requirement
        }
    }
}
mod scoring_accuracy_level {
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub enum ScoringAccuracyLevel {
        #[default]
        BasicApproximation,
        ProfileSpecificExact,
        Unsupported,
        InsufficientTrace,
    }

    impl ScoringAccuracyLevel {
        pub fn parse(value: &str) -> Option<Self> {
            match value.to_ascii_lowercase().as_str() {
                "basic-approximation" | "basic" | "approx" | "approximate" => {
                    Some(Self::BasicApproximation)
                }
                "profile-specific-exact" | "exact" => Some(Self::ProfileSpecificExact),
                "unsupported" => Some(Self::Unsupported),
                "insufficient-trace" => Some(Self::InsufficientTrace),
                _ => None,
            }
        }
    }
    impl ScoringAccuracyLevel {
        pub fn as_str(self) -> &'static str {
            match self {
                Self::BasicApproximation => "basic-approximation",
                Self::ProfileSpecificExact => "profile-specific-exact",
                Self::Unsupported => "unsupported",
                Self::InsufficientTrace => "insufficient-trace",
            }
        }
    }
}
#[cfg(test)]
use super::{
    AllSpinScoreMapping, DropScorePolicy, LevelPolicy, PcBonusPolicy, SpinAwardPolicy, SpinRuleId,
    TraceRequirement,
};

pub use attack_model_id::AttackModelId;
pub use b2b_policy::{B2BChainRule, B2BPolicy, B2BScoreRule};
pub use combo_policy::ComboPolicy;
pub use score_model_id::ScoreModelId;
pub use score_profile_model::{ScoreProfile, BASIC_APPROXIMATION_REASON};
pub use scoring_accuracy_level::ScoringAccuracyLevel;

#[cfg(test)]
#[path = "score_profile_tests.rs"]
mod tests;
