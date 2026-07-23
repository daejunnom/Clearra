use clearra_spin::SpinAwardProfile;

use super::{AllSpinScoreMapping, SpinAwardPolicy};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TSpinRecognition {
    Disabled,
    Simple,
    ThreeCorner,
    ThreeCornerOrImmobileMini,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NonTSpinRecognition {
    Disabled,
    ImmobileRegular,
    ImmobileMini,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum SpinProfileId {
    #[default]
    Disabled,
    TSpinSimple,
    TSpins,
    TSpinsPlus,
    AllSpin,
    AllSpinPlus,
    AllMini,
    AllMiniPlus,
}

impl SpinProfileId {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "disabled" | "none" => Some(Self::Disabled),
            "t-spin-simple" => Some(Self::TSpinSimple),
            "t-spin" | "t-spins" | "t-spin-corner" | "t-spin-corner-based" | "t-spin-3-corner" => {
                Some(Self::TSpins)
            }
            "t-spin-plus" | "t-spins-plus" => Some(Self::TSpinsPlus),
            "all-spin" | "all-spins" => Some(Self::AllSpin),
            "all-spin-plus" | "all-spins-plus" | "all-plus" => Some(Self::AllSpinPlus),
            "all-mini" => Some(Self::AllMini),
            "all-mini-plus" | "srs-plus-all-mini" => Some(Self::AllMiniPlus),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::TSpinSimple => "t-spin-simple",
            Self::TSpins => "t-spins",
            Self::TSpinsPlus => "t-spins-plus",
            Self::AllSpin => "all-spin",
            Self::AllSpinPlus => "all-spin-plus",
            Self::AllMini => "all-mini",
            Self::AllMiniPlus => "all-mini-plus",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Disabled => "Disabled",
            Self::TSpinSimple => "T-Spin simple",
            Self::TSpins => "T-Spins",
            Self::TSpinsPlus => "T-Spins+",
            Self::AllSpin => "All-Spin",
            Self::AllSpinPlus => "All-Spin+",
            Self::AllMini => "All-Mini",
            Self::AllMiniPlus => "All-Mini+",
        }
    }

    pub const fn allows_immobile_t_fallback(self) -> bool {
        SpinProfile::builtin(self).allows_immobile_t_fallback()
    }

    pub const fn recognizes_non_t_immobile_spins(self) -> bool {
        SpinProfile::builtin(self).recognizes_non_t_immobile_spins()
    }

    pub const fn requires_all_piece_evidence(self) -> bool {
        self.recognizes_non_t_immobile_spins()
    }

    pub const fn non_t_spins_score_as_t_spin_mini(self) -> bool {
        matches!(
            SpinProfile::builtin(self).non_t_spin_recognition(),
            NonTSpinRecognition::ImmobileMini
        )
    }

    #[allow(non_upper_case_globals)]
    pub const TSpinCornerBased: Self = Self::TSpins;

    #[allow(non_upper_case_globals)]
    pub const SrsPlusAllMini: Self = Self::AllMiniPlus;
}

pub type SpinRuleId = SpinProfileId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpinProfile {
    id: SpinProfileId,
    t_spin_recognition: TSpinRecognition,
    non_t_spin_recognition: NonTSpinRecognition,
    award_policy: SpinAwardPolicy,
    all_spin_score_mapping: AllSpinScoreMapping,
}

impl SpinProfile {
    pub const fn builtin(id: SpinProfileId) -> Self {
        let (t_spin_recognition, non_t_spin_recognition, award_policy, all_spin_score_mapping) =
            match id {
                SpinProfileId::Disabled => (
                    TSpinRecognition::Disabled,
                    NonTSpinRecognition::Disabled,
                    SpinAwardPolicy::Disabled,
                    AllSpinScoreMapping::Disabled,
                ),
                SpinProfileId::TSpinSimple => (
                    TSpinRecognition::Simple,
                    NonTSpinRecognition::Disabled,
                    SpinAwardPolicy::TSpinsOnly,
                    AllSpinScoreMapping::Disabled,
                ),
                SpinProfileId::TSpins => (
                    TSpinRecognition::ThreeCorner,
                    NonTSpinRecognition::Disabled,
                    SpinAwardPolicy::TSpinsOnly,
                    AllSpinScoreMapping::Disabled,
                ),
                SpinProfileId::TSpinsPlus => (
                    TSpinRecognition::ThreeCornerOrImmobileMini,
                    NonTSpinRecognition::Disabled,
                    SpinAwardPolicy::TSpinsOnly,
                    AllSpinScoreMapping::Disabled,
                ),
                SpinProfileId::AllSpin => (
                    TSpinRecognition::ThreeCorner,
                    NonTSpinRecognition::ImmobileRegular,
                    SpinAwardPolicy::AllSpins,
                    AllSpinScoreMapping::NativeAllSpinTable,
                ),
                SpinProfileId::AllSpinPlus => (
                    TSpinRecognition::ThreeCornerOrImmobileMini,
                    NonTSpinRecognition::ImmobileRegular,
                    SpinAwardPolicy::AllSpins,
                    AllSpinScoreMapping::NativeAllSpinTable,
                ),
                SpinProfileId::AllMini => (
                    TSpinRecognition::ThreeCorner,
                    NonTSpinRecognition::ImmobileMini,
                    SpinAwardPolicy::AllMini,
                    AllSpinScoreMapping::UseTSpinMiniTable,
                ),
                SpinProfileId::AllMiniPlus => (
                    TSpinRecognition::ThreeCornerOrImmobileMini,
                    NonTSpinRecognition::ImmobileMini,
                    SpinAwardPolicy::AllMini,
                    AllSpinScoreMapping::UseTSpinMiniTable,
                ),
            };
        Self {
            id,
            t_spin_recognition,
            non_t_spin_recognition,
            award_policy,
            all_spin_score_mapping,
        }
    }

    pub const fn id(self) -> SpinProfileId {
        self.id
    }

    pub const fn award_policy(self) -> SpinAwardPolicy {
        self.award_policy
    }

    pub const fn t_spin_recognition(self) -> TSpinRecognition {
        self.t_spin_recognition
    }

    pub const fn non_t_spin_recognition(self) -> NonTSpinRecognition {
        self.non_t_spin_recognition
    }

    pub const fn allows_immobile_t_fallback(self) -> bool {
        matches!(
            self.t_spin_recognition,
            TSpinRecognition::ThreeCornerOrImmobileMini
        )
    }

    pub const fn recognizes_non_t_immobile_spins(self) -> bool {
        matches!(
            self.non_t_spin_recognition,
            NonTSpinRecognition::ImmobileRegular | NonTSpinRecognition::ImmobileMini
        )
    }

    pub fn requires_complete_movement_evidence(self, piece: char) -> bool {
        if piece.eq_ignore_ascii_case(&'T') {
            !matches!(
                self.t_spin_recognition,
                TSpinRecognition::Disabled | TSpinRecognition::Simple
            )
        } else {
            self.recognizes_non_t_immobile_spins()
        }
    }

    pub const fn all_spin_score_mapping(self) -> AllSpinScoreMapping {
        self.all_spin_score_mapping
    }

    pub fn award_profile(self) -> SpinAwardProfile {
        match self.all_spin_score_mapping {
            AllSpinScoreMapping::UseTSpinMiniTable => SpinAwardProfile::all_piece_as_t_spin_mini(),
            AllSpinScoreMapping::Disabled | AllSpinScoreMapping::NativeAllSpinTable => {
                SpinAwardProfile::standard()
            }
        }
    }

    pub const fn with_award_policy(mut self, award_policy: SpinAwardPolicy) -> Self {
        self.award_policy = award_policy;
        self
    }

    pub const fn with_all_spin_score_mapping(
        mut self,
        all_spin_score_mapping: AllSpinScoreMapping,
    ) -> Self {
        self.all_spin_score_mapping = all_spin_score_mapping;
        self
    }
}

impl Default for SpinProfile {
    fn default() -> Self {
        Self::builtin(SpinProfileId::Disabled)
    }
}
