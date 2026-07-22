use super::{
    kick_evidence_requirement::KickEvidenceRequirement,
    special_spin_case_id::SpecialSpinCaseId,
    spin_accuracy::SpinAccuracy,
    spin_classification::{ClassificationConfidence, SpinClassification},
    spin_classification_input::{KickEvidence, SpinClassificationInput},
    spin_result::{SpinKind, SpinResult},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SpecialSpinVerificationState {
    SourcePinnedFixture,
    VerifiedImport,
    DescriptorOnly,
    #[default]
    Disabled,
}

impl SpecialSpinVerificationState {
    pub fn enables_exact_classification(self) -> bool {
        matches!(self, Self::SourcePinnedFixture | Self::VerifiedImport)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpecialSpinCase {
    id: SpecialSpinCaseId,
    display_name: String,
    piece: char,
    required_kick_signature: Option<String>,
    board_signature_predicate: String,
    corner_rule_override: Option<String>,
    mini_override: Option<bool>,
    regular_override: Option<bool>,
    allowed_profiles: Vec<String>,
    verification_state: SpecialSpinVerificationState,
    kick_evidence_requirement: KickEvidenceRequirement,
}

impl SpecialSpinCase {
    pub fn new(id: SpecialSpinCaseId, display_name: impl Into<String>, piece: char) -> Self {
        Self {
            id,
            display_name: display_name.into(),
            piece: piece.to_ascii_uppercase(),
            required_kick_signature: None,
            board_signature_predicate: "source-pinned-board-signature-required".to_owned(),
            corner_rule_override: None,
            mini_override: None,
            regular_override: None,
            allowed_profiles: Vec::new(),
            verification_state: SpecialSpinVerificationState::Disabled,
            kick_evidence_requirement: KickEvidenceRequirement::RequiredForExact,
        }
    }
}
impl SpecialSpinCase {
    pub fn fin_descriptor() -> Self {
        Self::new(SpecialSpinCaseId::Fin, "Fin", 'T')
            .with_corner_rule_override("force-regular")
            .with_mini_override(false)
            .with_regular_override(true)
            .with_verification_state(SpecialSpinVerificationState::DescriptorOnly)
    }
}
impl SpecialSpinCase {
    pub fn iso_descriptor() -> Self {
        Self::new(SpecialSpinCaseId::Iso, "ISO", 'T')
            .with_verification_state(SpecialSpinVerificationState::DescriptorOnly)
    }
}
impl SpecialSpinCase {
    pub fn neo_descriptor() -> Self {
        Self::new(SpecialSpinCaseId::Neo, "NEO", 'T')
            .with_corner_rule_override("force-mini")
            .with_mini_override(true)
            .with_regular_override(false)
            .with_verification_state(SpecialSpinVerificationState::DescriptorOnly)
    }
}
impl SpecialSpinCase {
    pub fn with_verification_state(
        mut self,
        verification_state: SpecialSpinVerificationState,
    ) -> Self {
        self.verification_state = verification_state;
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
    pub fn with_required_kick_signature(mut self, signature: impl Into<String>) -> Self {
        self.required_kick_signature = Some(signature.into());
        self
    }
}
impl SpecialSpinCase {
    pub fn with_board_signature_predicate(mut self, predicate: impl Into<String>) -> Self {
        self.board_signature_predicate = predicate.into();
        self
    }
}
impl SpecialSpinCase {
    pub fn with_mini_override(mut self, mini: bool) -> Self {
        self.mini_override = Some(mini);
        self
    }
}
impl SpecialSpinCase {
    pub fn with_regular_override(mut self, regular: bool) -> Self {
        self.regular_override = Some(regular);
        self
    }
}
impl SpecialSpinCase {
    pub fn with_corner_rule_override(mut self, override_id: impl Into<String>) -> Self {
        self.corner_rule_override = Some(override_id.into());
        self
    }
}
impl SpecialSpinCase {
    pub fn id(&self) -> &SpecialSpinCaseId {
        &self.id
    }
}
impl SpecialSpinCase {
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
}
impl SpecialSpinCase {
    pub fn piece(&self) -> char {
        self.piece
    }
}
impl SpecialSpinCase {
    pub fn verification_state(&self) -> SpecialSpinVerificationState {
        self.verification_state
    }
}
impl SpecialSpinCase {
    pub fn kick_evidence_requirement(&self) -> KickEvidenceRequirement {
        self.kick_evidence_requirement
    }
}
impl SpecialSpinCase {
    pub fn exact_enabled(&self, kick_evidence_available: bool) -> bool {
        self.verification_state.enables_exact_classification()
            && (!self.kick_evidence_requirement.requires_evidence() || kick_evidence_available)
    }
}
impl SpecialSpinCase {
    pub fn disabled_reason(&self, kick_evidence_available: bool) -> Option<&'static str> {
        if !self.verification_state.enables_exact_classification() {
            return Some("special_spin_profile_unverified");
        }
        if self.kick_evidence_requirement.requires_evidence() && !kick_evidence_available {
            return Some("spin_kick_evidence_missing");
        }
        None
    }
}
impl SpecialSpinCase {
    pub fn allowed_profiles(&self) -> &[String] {
        &self.allowed_profiles
    }
}
impl SpecialSpinCase {
    pub fn board_signature_predicate(&self) -> &str {
        &self.board_signature_predicate
    }
}
impl SpecialSpinCase {
    pub fn required_kick_signature(&self) -> Option<&str> {
        self.required_kick_signature.as_deref()
    }
}
impl SpecialSpinCase {
    pub fn corner_rule_override(&self) -> Option<&str> {
        self.corner_rule_override.as_deref()
    }
}
impl SpecialSpinCase {
    pub fn mini_override(&self) -> Option<bool> {
        self.mini_override
    }
}
impl SpecialSpinCase {
    pub fn regular_override(&self) -> Option<bool> {
        self.regular_override
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
    pub fn required_kick_signature_matches(&self, evidence: &KickEvidence) -> bool {
        self.required_kick_signature
            .as_deref()
            .is_none_or(|required| {
                required == "any"
                    || required == evidence.stable_signature()
                    || required == kick_signature_without_table(evidence)
            })
    }
}
impl SpecialSpinCase {
    pub fn board_signature_matches(&self, input: &SpinClassificationInput) -> bool {
        match self.board_signature_predicate.as_str() {
            "any" | "source-pinned-fixture" => true,
            "blocked-corners>=3" | "t-three-corner" => input.blocked_corners >= 3,
            "board-before-nonzero" => input.board_before != 0,
            "after-placement-changed" => input.board_after_placement != input.board_before,
            "source-pinned-board-signature-required" => false,
            _ => false,
        }
    }
}
impl SpecialSpinCase {
    pub fn classify(&self, input: &SpinClassificationInput) -> SpinClassification {
        let mini = self.mini_override.unwrap_or(false);
        let spin_kind = if mini {
            SpinKind::ProfileSpecific(self.id.profile_specific_kind_id())
        } else {
            SpinKind::ProfileSpecific(self.id.profile_specific_kind_id())
        };
        let regular_allowed = self.regular_override.unwrap_or(true);
        if !mini && !regular_allowed {
            return SpinClassification::new(
                SpinResult::none(input.piece, input.cleared_lines, SpinAccuracy::Incomplete),
                ClassificationConfidence::new(0.0),
            );
        }

        SpinClassification::new(
            SpinResult::new(
                input.piece,
                spin_kind,
                mini,
                input.cleared_lines,
                true,
                SpinAccuracy::Exact,
            ),
            ClassificationConfidence::exact(),
        )
    }
}

fn kick_signature_without_table(evidence: &KickEvidence) -> String {
    format!(
        "from={};to={};request={};kick={};dx={};dy={}",
        evidence.from_rotation,
        evidence.to_rotation,
        evidence.rotation_request.as_str(),
        evidence.kick_index,
        evidence.kick_dx,
        evidence.kick_dy
    )
}
