use clearra_scoring::profile::ScoringAccuracyLevel;
use clearra_scoring::profile::{AllSpinScoreMapping, SpinAwardPolicy};

use crate::{
    diagnostic::{diagnostic_code::DiagnosticCode, diagnostic_report::DiagnosticReport},
    evidence::validation_evidence::ValidationEvidence,
};

use super::{
    score_profile_object_diagnostic_builder::score_object_diagnostic,
    score_profile_object_registry_validator::{
        is_tetrio_style_default_profile, spin_classifier_supports_all_piece,
    },
    score_profile_object_validator::ScoreProfileObjectDescriptor,
};

pub(crate) fn validate_drop_score_trace_contract(
    object: &ScoreProfileObjectDescriptor,
    report: &mut DiagnosticReport,
) {
    if !object.drop_score_policy().requires_drop_events() || object.trace_completeness().is_full() {
        return;
    }

    report.push(
        score_object_diagnostic(
            object,
            DiagnosticCode::EScoreProfileSpinPolicyIncompatible,
            "drop-score policy requires full trace completeness with drop events",
            "drop_score_requires_trace_completeness",
        )
        .with_evidence(ValidationEvidence::new(
            "trace_completeness",
            format!("{:?}", object.trace_completeness()),
        ))
        .with_suggested_next_step(
            crate::diagnostic::suggested_next_step::SuggestedNextStep::new(
                "Use full replay traces for drop-score profiles, or disable drop-score evaluation.",
            ),
        ),
    );
}

pub(crate) fn validate_profile_specific_exact_contract(
    object: &ScoreProfileObjectDescriptor,
    report: &mut DiagnosticReport,
) {
    if object.accuracy_level() != ScoringAccuracyLevel::ProfileSpecificExact {
        return;
    }

    if object.exact_score_table_pinned()
        && object.exact_spin_classifier_available()
        && object.trace_completeness().is_full()
        && object.drop_score_basis_sufficient()
        && object.profile_specific_fixtures_pass()
    {
        return;
    }

    report.push(
        score_object_diagnostic(
            object,
            DiagnosticCode::EScoreProfileInvalid,
            "profile-specific exact scoring requires pinned exact tables, exact spin classifier, full trace basis, sufficient drop-score basis, and passing profile fixtures",
            "profile_specific_exact_requires_exact_basis",
        )
        .with_evidence(ValidationEvidence::new(
            "exact_score_table_pinned",
            object.exact_score_table_pinned().to_string(),
        ))
        .with_evidence(ValidationEvidence::new(
            "exact_spin_classifier_available",
            object.exact_spin_classifier_available().to_string(),
        ))
        .with_evidence(ValidationEvidence::new(
            "drop_score_basis_sufficient",
            object.drop_score_basis_sufficient().to_string(),
        ))
        .with_evidence(ValidationEvidence::new(
            "profile_specific_fixtures_pass",
            object.profile_specific_fixtures_pass().to_string(),
        )),
    );
}

pub(crate) fn validate_default_all_spin_policy(
    object: &ScoreProfileObjectDescriptor,
    report: &mut DiagnosticReport,
) {
    if !is_tetrio_style_default_profile(object.profile_id()) {
        return;
    }

    if !object.spin_award_policy().allows_all_spins()
        && object.all_spin_score_mapping() == AllSpinScoreMapping::Disabled
    {
        return;
    }

    report.push(score_object_diagnostic(
        object,
        DiagnosticCode::EScoreProfileSpinPolicyIncompatible,
        "TETR.IO-style default profiles must not enable AllSpin scoring by default",
        "all_spin_default_forbidden_for_tetrio_score",
    ));
}

pub(crate) fn validate_all_spin_classifier_contract(
    object: &ScoreProfileObjectDescriptor,
    report: &mut DiagnosticReport,
) {
    if !object.spin_award_policy().requires_all_piece_classifier()
        && !object
            .all_spin_score_mapping()
            .requires_all_piece_classifier()
    {
        return;
    }

    if spin_classifier_supports_all_piece(object.spin_classifier_id()) {
        return;
    }

    report.push(
        score_object_diagnostic(
            object,
            DiagnosticCode::EScoreProfileSpinPolicyIncompatible,
            "AllSpin score policy requires an all-piece spin classifier",
            "all_spin_requires_all_piece_classifier",
        )
        .with_evidence(ValidationEvidence::new(
            "spin_classifier_id",
            object.spin_classifier_id(),
        )),
    );
}

pub(crate) fn validate_all_mini_classifier_contract(
    object: &ScoreProfileObjectDescriptor,
    report: &mut DiagnosticReport,
) {
    if !object.all_mini_policy_enabled()
        && object.spin_award_policy() != SpinAwardPolicy::AllMini
        && object.spin_award_policy() != SpinAwardPolicy::AllSpinAsTSpinMini
        && object.all_spin_score_mapping() != AllSpinScoreMapping::UseTSpinMiniTable
    {
        return;
    }

    if spin_classifier_supports_all_piece(object.spin_classifier_id()) {
        return;
    }

    report.push(
        score_object_diagnostic(
            object,
            DiagnosticCode::EScoreProfileSpinPolicyIncompatible,
            "AllMini policy requires an all-piece spin classifier",
            "all_mini_requires_all_piece_classifier",
        )
        .with_evidence(ValidationEvidence::new(
            "spin_classifier_id",
            object.spin_classifier_id(),
        )),
    );
}
