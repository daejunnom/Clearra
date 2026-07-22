use crate::{board::board_profile::BoardProfileId, pieces::piece_set_profile::PieceSetProfileId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RandomizerProfileId(String);

impl RandomizerProfileId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RotationProfileId(String);

impl RotationProfileId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KickProfileId(String);

impl KickProfileId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinRecognitionProfileId(String);

impl SpinRecognitionProfileId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinResolutionProfileId(String);

impl SpinResolutionProfileId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinAwardProfileId(String);

impl SpinAwardProfileId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoreProfileId(String);

impl ScoreProfileId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttackProfileId(String);

impl AttackProfileId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReplayProfileId(String);

impl ReplayProfileId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProfileCapabilityReport {
    exact_spin: bool,
    exact_score: bool,
    exact_attack: bool,
    disabled_reason: Option<String>,
}

impl ProfileCapabilityReport {
    pub fn unsupported(disabled_reason: impl Into<String>) -> Self {
        Self {
            exact_spin: false,
            exact_score: false,
            exact_attack: false,
            disabled_reason: Some(disabled_reason.into()),
        }
    }
}
impl ProfileCapabilityReport {
    pub fn exact_spin(self) -> bool {
        self.exact_spin
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameRulesetProfile {
    pub board_profile: BoardProfileId,
    pub piece_set_profile: PieceSetProfileId,
    pub randomizer_profile: RandomizerProfileId,
    pub rotation_profile: RotationProfileId,
    pub kick_profile: KickProfileId,
    pub spin_recognition_profile: SpinRecognitionProfileId,
    pub spin_resolution_profile: SpinResolutionProfileId,
    pub spin_award_profile: SpinAwardProfileId,
    pub score_profile: ScoreProfileId,
    pub attack_profile: AttackProfileId,
    pub replay_profile: ReplayProfileId,
    pub capability_report: ProfileCapabilityReport,
}

impl GameRulesetProfile {
    pub fn standard_unsupported() -> Self {
        Self {
            board_profile: BoardProfileId::Standard10,
            piece_set_profile: PieceSetProfileId::StandardTetrominoes,
            randomizer_profile: RandomizerProfileId::new("standard-7-bag"),
            rotation_profile: RotationProfileId::new("srs"),
            kick_profile: KickProfileId::new("srs-plus"),
            spin_recognition_profile: SpinRecognitionProfileId::new("t-spin-corner-recognition"),
            spin_resolution_profile: SpinResolutionProfileId::new(
                "preserve-all-legal-spin-interpretations",
            ),
            spin_award_profile: SpinAwardProfileId::new("standard-spin-award"),
            score_profile: ScoreProfileId::new("tetrio"),
            attack_profile: AttackProfileId::new("tetrio"),
            replay_profile: ReplayProfileId::new("lock-frame-replay"),
            capability_report: ProfileCapabilityReport::unsupported(
                "profile_pipeline_not_connected",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_ruleset_profile_is_thin_bundle_over_split_profiles() {
        let profile = GameRulesetProfile::standard_unsupported();

        assert_eq!(profile.board_profile, BoardProfileId::Standard10);
        assert_eq!(
            profile.piece_set_profile,
            PieceSetProfileId::StandardTetrominoes
        );
        assert!(!profile.capability_report.exact_spin());
    }
}
