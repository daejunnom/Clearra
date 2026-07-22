use clearra_supply::{
    bag::bag_boundary::BagBoundaryReport,
    custom_bag::CustomBagRuntimeGuard,
    diagnostics::duplicate_witness::DuplicateWitness,
    mixed::CustomBagProfile,
    normalize::ambiguity_report::{AmbiguityReason, AmbiguityReport},
    queue::fixed_sequence::FixedSequence,
};

use crate::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        suggested_next_step::SuggestedNextStep,
    },
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
};

pub(super) fn duplicate_diagnostic(witness: DuplicateWitness, path: &'static str) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::ESupplyInvalidDuplicate,
        "queue contains a duplicate piece within the same inferred bag segment",
    )
    .with_location(EvidenceLocation::with_index(
        path,
        witness.duplicate_index(),
    ))
    .with_evidence(ValidationEvidence::new(
        "piece",
        witness.piece().as_ascii().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "first_index",
        witness.first_index().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "duplicate_index",
        witness.duplicate_index().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "initial_offset",
        witness.initial_offset().to_string(),
    ))
    .with_suggested_next_step(SuggestedNextStep::new(
        "Review the queue string or mark the input as observed if the bag boundary is unknown.",
    ))
}

pub(super) fn ambiguity_diagnostic(ambiguity: &AmbiguityReport) -> Diagnostic {
    let reason = match ambiguity.reason() {
        AmbiguityReason::EmptyObservedWindow => "empty_observed_window",
        AmbiguityReason::MultipleBoundaryCandidates => "multiple_boundary_candidates",
    };

    Diagnostic::new(
        DiagnosticCode::WSupplyAmbiguousObservedWindow,
        "observed queue is compatible with multiple bag boundary offsets",
    )
    .with_location(EvidenceLocation::new("supply.observed_queue"))
    .with_evidence(ValidationEvidence::new("reason", reason))
    .with_evidence(ValidationEvidence::new(
        "observed_len",
        ambiguity.observed_len().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "candidate_count",
        ambiguity.candidates().len().to_string(),
    ))
    .with_suggested_next_step(SuggestedNextStep::new(
        "Provide more observed pieces or use a fixed queue when the exact boundary is known.",
    ))
}

pub(super) fn fixed_sequence_diagnostic(
    sequence: &FixedSequence,
    path: &'static str,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::ISupplyFixedSequenceAccepted,
        "fixed sequence is treated as an exact next queue; bag boundary compatibility is not enforced",
    )
    .with_location(EvidenceLocation::new(path))
    .with_evidence(ValidationEvidence::new(
        "queue_len",
        sequence.len().to_string(),
    ))
}

pub(super) fn boundary_compatible_diagnostic(
    boundary_report: &BagBoundaryReport,
    path: &'static str,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::ISupplyBoundaryCompatible,
        "queue is compatible with the configured bag boundary model",
    )
    .with_location(EvidenceLocation::new(path))
    .with_evidence(ValidationEvidence::new(
        "bag_size",
        boundary_report.bag_size().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "candidate_count",
        boundary_report.candidates().len().to_string(),
    ))
}

pub(super) fn custom_bag_runtime_guard_diagnostic(bag_profile: &CustomBagProfile) -> Diagnostic {
    custom_bag_runtime_guard_diagnostic_from_guard(
        &CustomBagRuntimeGuard::from_profile(bag_profile),
        bag_profile.custom_bag_schema_valid(),
        bag_profile.bag_size(),
        bag_profile.total_weight(),
    )
}

pub(super) fn custom_bag_runtime_guard_diagnostic_from_guard(
    guard: &CustomBagRuntimeGuard,
    custom_bag_schema_valid: bool,
    bag_size: usize,
    total_weight: u32,
) -> Diagnostic {
    custom_bag_runtime_guard_diagnostic_from_parts(
        guard.bag_profile_id(),
        guard.piece_set_id(),
        custom_bag_schema_valid,
        bag_size,
        total_weight,
        guard.disabled_reason(),
    )
}

pub(super) fn custom_bag_runtime_guard_diagnostic_from_parts(
    bag_profile_id: &str,
    piece_set_id: &str,
    custom_bag_schema_valid: bool,
    bag_size: usize,
    total_weight: u32,
    reason: &'static str,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::ECustomBagUnsupportedMvp,
        "custom bag schema is valid but custom bag runtime is not connected",
    )
    .with_location(EvidenceLocation::new("supply.custom_bag_profile"))
    .with_evidence(ValidationEvidence::new(
        "custom_bag_schema_valid",
        custom_bag_schema_valid.to_string(),
    ))
    .with_evidence(ValidationEvidence::new("bag_profile_id", bag_profile_id))
    .with_evidence(ValidationEvidence::new("piece_set_id", piece_set_id))
    .with_evidence(ValidationEvidence::new("bag_size", bag_size.to_string()))
    .with_evidence(ValidationEvidence::new(
        "total_weight",
        total_weight.to_string(),
    ))
    .with_evidence(ValidationEvidence::new("reason", reason))
    .with_suggested_next_step(SuggestedNextStep::new(
        "Keep custom bag profiles as schema fixtures until mixed/custom supply runtime is enabled.",
    ))
}
