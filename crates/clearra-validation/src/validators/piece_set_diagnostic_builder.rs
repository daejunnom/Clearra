use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_piece_registry::registry::{MixedBagProfile, MixedPieceSet};

use crate::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        suggested_next_step::SuggestedNextStep,
    },
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
};

pub(super) fn standard_piece_set_supported_diagnostic(
    location: &'static str,
    piece_count: usize,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::IPieceSetMvpSupported,
        "standard seven tetromino piece set is supported in MVP1",
    )
    .with_location(EvidenceLocation::new(location))
    .with_evidence(ValidationEvidence::new(
        "piece_count",
        piece_count.to_string(),
    ))
}

pub(super) fn standard_piece_set_unsupported_diagnostic(
    location: &'static str,
    pieces: &[PieceKind],
    reason: String,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::EPieceSetUnsupportedMvp,
        "only the standard seven tetromino piece set is supported in MVP1",
    )
    .with_location(EvidenceLocation::new(location))
    .with_evidence(ValidationEvidence::new("reason", reason))
    .with_evidence(ValidationEvidence::new(
        "piece_count",
        pieces.len().to_string(),
    ))
    .with_suggested_next_step(SuggestedNextStep::new(
        "Use the standard I/O/T/S/Z/J/L piece set.",
    ))
}

pub(super) fn oversized_piece_budget_diagnostic(max_piece_count: usize) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::EPieceSetUnsupportedMvp,
        "setup piece budget exceeds the MVP1 standard 7-bag depth",
    )
    .with_location(EvidenceLocation::new("setup.piece_budget.max_piece_count"))
    .with_evidence(ValidationEvidence::new(
        "max_piece_count",
        max_piece_count.to_string(),
    ))
}

pub(super) fn custom_piece_runtime_guard_diagnostic(
    piece_set: &MixedPieceSet,
    custom_ids: String,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::ECustomPieceUnsupportedMvp,
        "custom and mixed piece sets have an MVP3 schema but are not connected to search runtime",
    )
    .with_location(EvidenceLocation::new("pieces.mixed_registry"))
    .with_evidence(ValidationEvidence::new("piece_set_id", piece_set.id()))
    .with_evidence(ValidationEvidence::new(
        "piece_count",
        piece_set.len().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "custom_piece_count",
        piece_set.custom_piece_count().to_string(),
    ))
    .with_evidence(ValidationEvidence::new("custom_piece_ids", custom_ids))
    .with_evidence(ValidationEvidence::new(
        "reason",
        "custom_piece_runtime_not_connected",
    ))
    .with_evidence(ValidationEvidence::new(
        "mixed_piece_reason",
        "mixed_piece_runtime_not_connected",
    ))
    .with_evidence(ValidationEvidence::new(
        "candidate_runtime_reason",
        "custom_candidate_runtime_unsupported",
    ))
    .with_evidence(ValidationEvidence::new(
        "reachability_runtime_reason",
        "custom_reachability_runtime_unsupported",
    ))
    .with_suggested_next_step(SuggestedNextStep::new(
        "Keep custom piece definitions in fixtures/schema until the generalized placement/search runtime is enabled.",
    ))
}

pub(super) fn standard_only_mixed_piece_set_supported_diagnostic(
    piece_set: &MixedPieceSet,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::IPieceSetMvpSupported,
        "mixed piece registry contains only standard tetromino definitions supported by MVP runtime",
    )
    .with_location(EvidenceLocation::new("pieces.mixed_registry"))
    .with_evidence(ValidationEvidence::new("piece_set_id", piece_set.id()))
    .with_evidence(ValidationEvidence::new(
        "piece_count",
        piece_set.len().to_string(),
    ))
}

pub(super) fn custom_bag_runtime_guard_diagnostic(
    piece_set: &MixedPieceSet,
    bag_profile: &MixedBagProfile,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::ECustomBagUnsupportedMvp,
        "custom bag profiles can be defined in MVP3 schema but are not connected to search runtime",
    )
    .with_location(EvidenceLocation::new("supply.mixed_bag_profile"))
    .with_evidence(ValidationEvidence::new("piece_set_id", piece_set.id()))
    .with_evidence(ValidationEvidence::new("bag_profile_id", bag_profile.id()))
    .with_evidence(ValidationEvidence::new(
        "bag_size",
        bag_profile.bag_size().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "total_weight",
        bag_profile.total_weight().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "reason",
        "custom_bag_runtime_not_connected",
    ))
    .with_suggested_next_step(SuggestedNextStep::new(
        "Keep custom bag profiles in fixtures/schema until PieceDefinitionId-based supply and placement runtime are enabled.",
    ))
}

pub(super) fn standard_mixed_bag_profile_supported_diagnostic(
    bag_profile: &MixedBagProfile,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::ISupplyBoundaryCompatible,
        "standard-piece bag profile can use generalized supply boundary validation",
    )
    .with_location(EvidenceLocation::new("supply.mixed_bag_profile"))
    .with_evidence(ValidationEvidence::new("bag_profile_id", bag_profile.id()))
    .with_evidence(ValidationEvidence::new(
        "bag_size",
        bag_profile.bag_size().to_string(),
    ))
}
