use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_replay::RotationRequest;

use crate::{evidence::KickEvidence, special::SpecialSpinCaseId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KickSignature {
    from_rotation: RotationState,
    to_rotation: RotationState,
    rotation_request: RotationRequest,
    kick_index: u8,
    kick_dx: i16,
    kick_dy: i16,
}

impl KickSignature {
    pub const fn new(
        from_rotation: RotationState,
        to_rotation: RotationState,
        rotation_request: RotationRequest,
        kick_index: u8,
        kick_dx: i16,
        kick_dy: i16,
    ) -> Self {
        Self {
            from_rotation,
            to_rotation,
            rotation_request,
            kick_index,
            kick_dx,
            kick_dy,
        }
    }
}
impl KickSignature {
    pub fn matches(&self, evidence: &KickEvidence) -> bool {
        evidence.from_rotation == self.from_rotation
            && evidence.to_rotation == self.to_rotation
            && evidence.rotation_request == self.rotation_request
            && evidence.kick_index == self.kick_index
            && evidence.kick_dx == self.kick_dx
            && evidence.kick_dy == self.kick_dy
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BoardSignaturePredicate {
    #[default]
    Any,
    Exact {
        board_before: u64,
        board_after_place: u64,
    },
}

impl BoardSignaturePredicate {
    pub const fn matches(self, board_before: u64, board_after_place: u64) -> bool {
        match self {
            Self::Any => true,
            Self::Exact {
                board_before: expected_before,
                board_after_place: expected_after,
            } => board_before == expected_before && board_after_place == expected_after,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CornerRuleOverride {
    #[default]
    None,
    ForceMini,
    ForceRegular,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SpecialSpinVerificationState {
    #[default]
    DescriptorOnly,
    VerifiedImport,
    SourcePinnedExact,
}

impl SpecialSpinVerificationState {
    pub const fn exact_enabled(self) -> bool {
        matches!(self, Self::VerifiedImport | Self::SourcePinnedExact)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpecialSpinCase {
    pub id: SpecialSpinCaseId,
    pub display_name: String,
    pub piece: PieceKind,
    pub required_kick_signature: Option<KickSignature>,
    pub board_signature_predicate: BoardSignaturePredicate,
    pub corner_rule_override: CornerRuleOverride,
    pub mini_override: Option<bool>,
    pub regular_override: Option<bool>,
    pub allowed_profiles: Vec<String>,
    pub verification_state: SpecialSpinVerificationState,
}

impl SpecialSpinCase {
    pub fn fin_descriptor() -> Self {
        let mut descriptor = Self::new(SpecialSpinCaseId::Fin, "Fin", PieceKind::T);
        descriptor.corner_rule_override = CornerRuleOverride::ForceRegular;
        descriptor.mini_override = Some(false);
        descriptor.regular_override = Some(true);
        descriptor
    }
}
impl SpecialSpinCase {
    pub fn iso_descriptor() -> Self {
        Self::new(SpecialSpinCaseId::Iso, "ISO", PieceKind::T)
    }
}
impl SpecialSpinCase {
    pub fn neo_descriptor() -> Self {
        let mut descriptor = Self::new(SpecialSpinCaseId::Neo, "NEO", PieceKind::T);
        descriptor.corner_rule_override = CornerRuleOverride::ForceMini;
        descriptor.mini_override = Some(true);
        descriptor.regular_override = Some(false);
        descriptor
    }
}
impl SpecialSpinCase {
    pub fn new(id: SpecialSpinCaseId, display_name: impl Into<String>, piece: PieceKind) -> Self {
        Self {
            id,
            display_name: display_name.into(),
            piece,
            required_kick_signature: None,
            board_signature_predicate: BoardSignaturePredicate::Any,
            corner_rule_override: CornerRuleOverride::None,
            mini_override: None,
            regular_override: None,
            allowed_profiles: Vec::new(),
            verification_state: SpecialSpinVerificationState::DescriptorOnly,
        }
    }
}
impl SpecialSpinCase {
    pub fn with_required_kick_signature(mut self, signature: KickSignature) -> Self {
        self.required_kick_signature = Some(signature);
        self
    }
}
impl SpecialSpinCase {
    pub fn with_board_signature_predicate(mut self, predicate: BoardSignaturePredicate) -> Self {
        self.board_signature_predicate = predicate;
        self
    }
}
impl SpecialSpinCase {
    pub fn with_allowed_profile(mut self, profile_id: impl Into<String>) -> Self {
        self.allowed_profiles.push(profile_id.into());
        self
    }
}
impl SpecialSpinCase {
    pub fn with_verification_state(mut self, state: SpecialSpinVerificationState) -> Self {
        self.verification_state = state;
        self
    }
}
impl SpecialSpinCase {
    pub fn allowed_for_profile(&self, profile_id: &str) -> bool {
        self.allowed_profiles.is_empty()
            || self
                .allowed_profiles
                .iter()
                .any(|allowed| allowed == profile_id)
    }
}
impl SpecialSpinCase {
    pub fn exact_enabled(&self) -> bool {
        self.verification_state.exact_enabled()
    }
}
impl SpecialSpinCase {
    pub fn exact_match(
        &self,
        profile_id: &str,
        kick_evidence: Option<&KickEvidence>,
        board_before: u64,
        board_after_place: u64,
    ) -> bool {
        if !self.exact_enabled() || !self.allowed_for_profile(profile_id) {
            return false;
        }
        if !self
            .board_signature_predicate
            .matches(board_before, board_after_place)
        {
            return false;
        }
        let Some(kick_evidence) = kick_evidence else {
            return false;
        };
        if !kick_evidence.has_exact_first_success() {
            return false;
        }
        self.required_kick_signature
            .as_ref()
            .is_none_or(|signature| signature.matches(kick_evidence))
    }
}
