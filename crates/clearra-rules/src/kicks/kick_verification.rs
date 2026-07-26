use clearra_core_domain::piece::piece_kind::PieceKind;

use super::{
    kick_table::{KickOffsetSequence, KickTableProfile, KickTransition},
    srs_offsets::{eight_direction_transitions, one_eighty_transitions},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KickVerificationCase {
    name: &'static str,
    transition: KickTransition,
    expected_sequence: KickOffsetSequence,
}

impl KickVerificationCase {
    pub fn new(
        name: &'static str,
        transition: KickTransition,
        expected_sequence: KickOffsetSequence,
    ) -> Self {
        Self {
            name,
            transition,
            expected_sequence,
        }
    }
}
impl KickVerificationCase {
    pub fn name(&self) -> &'static str {
        self.name
    }
}
impl KickVerificationCase {
    pub fn transition(&self) -> KickTransition {
        self.transition
    }
}
impl KickVerificationCase {
    pub fn expected_sequence(&self) -> &KickOffsetSequence {
        &self.expected_sequence
    }
}
impl KickVerificationCase {
    pub fn verify(&self, profile: &KickTableProfile) -> KickVerificationOutcome {
        match profile.sequence_for(self.transition) {
            Some(actual_sequence) if actual_sequence == &self.expected_sequence => {
                KickVerificationOutcome::Passed
            }
            Some(actual_sequence) => KickVerificationOutcome::Failed {
                reason: KickVerificationFailureReason::SequenceMismatch,
                actual_sequence: Some(actual_sequence.clone()),
            },
            None => KickVerificationOutcome::Failed {
                reason: KickVerificationFailureReason::MissingTransition,
                actual_sequence: None,
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KickVerificationOutcome {
    Passed,
    Failed {
        reason: KickVerificationFailureReason,
        actual_sequence: Option<KickOffsetSequence>,
    },
}

impl KickVerificationOutcome {
    pub fn is_passed(&self) -> bool {
        matches!(self, Self::Passed)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KickVerificationFailureReason {
    MissingTransition,
    SequenceMismatch,
}

impl KickVerificationFailureReason {
    pub fn code(self) -> &'static str {
        match self {
            Self::MissingTransition => "missing_transition",
            Self::SequenceMismatch => "sequence_mismatch",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KickProfileVerificationReport {
    issue_count: usize,
    missing_transition_count: usize,
    duplicate_transition_count: usize,
    unsupported_annotation_count: usize,
    supports_180: bool,
    transition_complete: bool,
}

impl KickProfileVerificationReport {
    pub fn verify_imported_profile(profile: &KickTableProfile) -> Self {
        let expected_transitions = expected_transitions(profile);
        let missing_transition_count = expected_transitions
            .iter()
            .filter(|transition| profile.sequence_for(**transition).is_none())
            .count();
        let duplicate_transition_count = profile.duplicate_transitions().len();
        let unsupported_annotation_count = profile
            .entries()
            .iter()
            .filter(|entry| entry.unsupported_reason().is_some())
            .count();
        let issue_count =
            missing_transition_count + duplicate_transition_count + unsupported_annotation_count;

        Self {
            issue_count,
            missing_transition_count,
            duplicate_transition_count,
            unsupported_annotation_count,
            supports_180: profile.supports_180(),
            transition_complete: missing_transition_count == 0,
        }
    }
}
impl KickProfileVerificationReport {
    pub fn issue_count(&self) -> usize {
        self.issue_count
    }
}
impl KickProfileVerificationReport {
    pub fn missing_transition_count(&self) -> usize {
        self.missing_transition_count
    }
}
impl KickProfileVerificationReport {
    pub fn duplicate_transition_count(&self) -> usize {
        self.duplicate_transition_count
    }
}
impl KickProfileVerificationReport {
    pub fn unsupported_annotation_count(&self) -> usize {
        self.unsupported_annotation_count
    }
}
impl KickProfileVerificationReport {
    pub fn supports_180(&self) -> bool {
        self.supports_180
    }
}
impl KickProfileVerificationReport {
    pub fn transition_complete(&self) -> bool {
        self.transition_complete
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedKickTableProfile {
    profile: KickTableProfile,
    report: KickProfileVerificationReport,
}

impl VerifiedKickTableProfile {
    pub fn try_new(profile: KickTableProfile) -> Result<Self, KickProfileVerificationReport> {
        let report = KickProfileVerificationReport::verify_imported_profile(&profile);
        if report.issue_count() > 0 {
            return Err(report);
        }
        Ok(Self { profile, report })
    }
}
impl VerifiedKickTableProfile {
    pub fn profile(&self) -> &KickTableProfile {
        &self.profile
    }
}
impl VerifiedKickTableProfile {
    pub fn report(&self) -> &KickProfileVerificationReport {
        &self.report
    }
}
impl VerifiedKickTableProfile {
    pub fn into_profile(self) -> KickTableProfile {
        self.profile
    }
}

fn expected_transitions(profile: &KickTableProfile) -> Vec<KickTransition> {
    let omit_o_rotations =
        profile.source_rule() == crate::profile::rule_profile::RuleProfileId::Jstris180;
    let base = PieceKind::STANDARD_TETROMINOES
        .into_iter()
        .filter(|piece| !omit_o_rotations || *piece != PieceKind::O)
        .flat_map(|piece| {
            eight_direction_transitions()
                .into_iter()
                .map(move |(from, to)| KickTransition::new(piece, from, to))
        });
    if !profile.supports_180() {
        return base.collect();
    }
    base.chain(
        PieceKind::STANDARD_TETROMINOES
            .into_iter()
            .filter(|piece| *piece != PieceKind::O)
            .flat_map(|piece| {
                one_eighty_transitions()
                    .into_iter()
                    .map(move |(from, to)| KickTransition::new(piece, from, to))
            }),
    )
    .collect()
}

#[cfg(test)]
#[path = "kick_verification_tests.rs"]
mod tests;
