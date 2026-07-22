use std::collections::HashSet;

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};

use crate::{
    kicks::{KickProfileVerificationReport, KickTransition},
    profile::rule_profile::RuleProfileId,
};

use super::{
    custom_rule_editor_schema::{CustomRuleEditorDraft, CustomRuleEditorSchema},
    custom_rule_runtime::CustomRuleRuntimeFeature,
};

pub type CustomRuleProfileVerificationReport = CustomRuleVerificationReport;
pub type CustomRuleProfileVerificationError = CustomRuleVerificationIssue;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomRuleVerificationReport {
    errors: Vec<CustomRuleVerificationIssue>,
    kick_report: KickProfileVerificationReport,
    missing_transition: usize,
    duplicate_transition: usize,
    invalid_rotation: usize,
    unsupported_piece: usize,
    unsupported_board_backend: usize,
    unsupported_runtime_feature: usize,
}

impl CustomRuleVerificationReport {
    pub fn verify_editor_draft(draft: &CustomRuleEditorDraft) -> Self {
        Self::verify_editor_schema(&CustomRuleEditorSchema::from_editor_draft(draft.clone()))
    }
}
impl CustomRuleVerificationReport {
    pub fn verify_editor_schema(schema: &CustomRuleEditorSchema) -> Self {
        let mut errors = Vec::new();
        if schema.id.trim().is_empty() {
            errors.push(CustomRuleVerificationIssue::EmptyRuleId);
        }
        if schema.label.trim().is_empty() {
            errors.push(CustomRuleVerificationIssue::EmptyRuleLabel);
        }
        if schema.kick_table_profile.source_rule() != RuleProfileId::Custom {
            errors.push(CustomRuleVerificationIssue::KickProfileMustTargetCustomRule);
        }
        if !schema.line_clear_policy.clears_full_rows() {
            errors.push(CustomRuleVerificationIssue::UnsupportedLineClearPolicy);
        }
        if schema.first_success_order.is_empty() {
            errors.push(CustomRuleVerificationIssue::MissingFirstSuccessOrder);
        }

        let invalid_rotation = schema
            .rotation_states
            .iter()
            .filter(|turns| RotationState::from_quarter_turns(**turns).is_err())
            .count();
        if invalid_rotation > 0 {
            errors.push(CustomRuleVerificationIssue::InvalidRotation);
        }

        let unsupported_piece = schema
            .spawn_rules
            .iter()
            .filter(|rule| rule.piece().is_none())
            .count()
            + schema
                .piece_specific_overrides
                .iter()
                .filter(|override_rule| override_rule.piece().is_none())
                .count();
        if unsupported_piece > 0 {
            errors.push(CustomRuleVerificationIssue::UnsupportedPiece);
        }

        let unsupported_board_backend = schema
            .board_backends
            .iter()
            .filter(|backend| !backend.runtime_supported())
            .count();
        if unsupported_board_backend > 0 || schema.board_backends.is_empty() {
            errors.push(CustomRuleVerificationIssue::UnsupportedBoardBackend);
        }

        let unsupported_runtime_feature = schema
            .runtime_features
            .iter()
            .filter(|feature| !feature.runtime_supported())
            .count()
            + usize::from(
                !schema
                    .runtime_features
                    .contains(&CustomRuleRuntimeFeature::CompactCDescriptor),
            )
            + usize::from(
                !schema
                    .runtime_features
                    .contains(&CustomRuleRuntimeFeature::StandardTetrominoPieces),
            );
        if unsupported_runtime_feature > 0 {
            errors.push(CustomRuleVerificationIssue::UnsupportedRuntimeFeature);
        }

        let kick_report =
            KickProfileVerificationReport::verify_imported_profile(&schema.kick_table_profile);
        let mut missing_transition = kick_report.missing_transition_count();
        if schema.supports_180 && !schema.kick_table_profile.supports_180() {
            missing_transition += PieceKind::STANDARD_TETROMINOES.len() * 4;
        }
        if missing_transition > 0 {
            errors.push(CustomRuleVerificationIssue::MissingTransition);
        }

        let duplicate_transition = kick_report.duplicate_transition_count()
            + duplicate_first_success_transition_count(&schema.first_success_order);
        if duplicate_transition > 0 {
            errors.push(CustomRuleVerificationIssue::DuplicateTransition);
        }

        Self {
            errors,
            kick_report,
            missing_transition,
            duplicate_transition,
            invalid_rotation,
            unsupported_piece,
            unsupported_board_backend,
            unsupported_runtime_feature,
        }
    }
}
impl CustomRuleVerificationReport {
    pub fn is_verified(&self) -> bool {
        self.errors.is_empty()
            && self.missing_transition == 0
            && self.duplicate_transition == 0
            && self.invalid_rotation == 0
            && self.unsupported_piece == 0
            && self.unsupported_board_backend == 0
            && self.unsupported_runtime_feature == 0
    }
}
impl CustomRuleVerificationReport {
    pub fn errors(&self) -> &[CustomRuleVerificationIssue] {
        &self.errors
    }
}
impl CustomRuleVerificationReport {
    pub fn issue_count(&self) -> usize {
        self.errors.len()
            + self.kick_report.issue_count()
            + self.invalid_rotation
            + self.unsupported_piece
            + self.unsupported_board_backend
            + self.unsupported_runtime_feature
    }
}
impl CustomRuleVerificationReport {
    pub fn kick_report(&self) -> &KickProfileVerificationReport {
        &self.kick_report
    }
}
impl CustomRuleVerificationReport {
    pub fn missing_transition(&self) -> usize {
        self.missing_transition
    }
}
impl CustomRuleVerificationReport {
    pub fn duplicate_transition(&self) -> usize {
        self.duplicate_transition
    }
}
impl CustomRuleVerificationReport {
    pub fn invalid_rotation(&self) -> usize {
        self.invalid_rotation
    }
}
impl CustomRuleVerificationReport {
    pub fn unsupported_piece(&self) -> usize {
        self.unsupported_piece
    }
}
impl CustomRuleVerificationReport {
    pub fn unsupported_board_backend(&self) -> usize {
        self.unsupported_board_backend
    }
}
impl CustomRuleVerificationReport {
    pub fn unsupported_runtime_feature(&self) -> usize {
        self.unsupported_runtime_feature
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CustomRuleVerificationIssue {
    EmptyRuleId,
    EmptyRuleLabel,
    KickProfileMustTargetCustomRule,
    UnverifiedKickProfile,
    UnsupportedLineClearPolicy,
    MissingTransition,
    DuplicateTransition,
    InvalidRotation,
    UnsupportedPiece,
    UnsupportedBoardBackend,
    UnsupportedRuntimeFeature,
    MissingFirstSuccessOrder,
}

impl CustomRuleVerificationIssue {
    pub fn code(self) -> &'static str {
        match self {
            Self::EmptyRuleId => "empty_rule_id",
            Self::EmptyRuleLabel => "empty_rule_label",
            Self::KickProfileMustTargetCustomRule => "kick_profile_must_target_custom_rule",
            Self::UnverifiedKickProfile => "unverified_kick_profile",
            Self::UnsupportedLineClearPolicy => "unsupported_line_clear_policy",
            Self::MissingTransition => "missing_transition",
            Self::DuplicateTransition => "duplicate_transition",
            Self::InvalidRotation => "invalid_rotation",
            Self::UnsupportedPiece => "unsupported_piece",
            Self::UnsupportedBoardBackend => "unsupported_board_backend",
            Self::UnsupportedRuntimeFeature => "unsupported_runtime_feature",
            Self::MissingFirstSuccessOrder => "first_success_order_missing",
        }
    }
}

fn duplicate_first_success_transition_count(transitions: &[KickTransition]) -> usize {
    let mut seen = HashSet::new();
    let mut duplicates = HashSet::new();
    for transition in transitions {
        if !seen.insert(*transition) {
            duplicates.insert(*transition);
        }
    }
    duplicates.len()
}
