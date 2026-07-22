use clearra_build_coverage::{
    domain::slot_domain::SlotDomain,
    query::build_coverage_query::BuildCoverageQuery,
    template::build_slot::{BuildSlot, BuildSlotId},
};

use crate::{
    diagnostic::diagnostic_report::DiagnosticReport,
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
    validators::build_query_validator::invalid_build_query,
};

pub(crate) fn validate_template_slot_domain(
    slot: &BuildSlot,
    index: usize,
    report: &mut DiagnosticReport,
) {
    if slot.allowed_pieces().is_empty() {
        report.push(
            invalid_build_query(
                "build.template.slots",
                "build template slot allowed piece domain must not be empty",
                "empty_template_slot_domain",
            )
            .with_location(EvidenceLocation::with_index("build.template.slots", index))
            .with_evidence(ValidationEvidence::new(
                "slot_id",
                slot.id().get().to_string(),
            )),
        );
    }

    if let Some(required_piece) = slot.required_piece() {
        if !slot.allowed_pieces().contains(&required_piece) {
            report.push(
                invalid_build_query(
                    "build.template.slots",
                    "build template required piece must be inside the slot allowed domain",
                    "template_required_piece_not_in_domain",
                )
                .with_location(EvidenceLocation::with_index("build.template.slots", index))
                .with_evidence(ValidationEvidence::new(
                    "slot_id",
                    slot.id().get().to_string(),
                ))
                .with_evidence(ValidationEvidence::new(
                    "piece",
                    required_piece.as_ascii().to_string(),
                )),
            );
        }
    }
}

pub(crate) fn validate_domains(query: &BuildCoverageQuery, report: &mut DiagnosticReport) {
    for slot in query.template().slots() {
        if domain_for_slot(query.domains(), slot.id()).is_none() {
            report.push(
                invalid_build_query(
                    "build.domains",
                    "each build template slot must have a slot domain",
                    "missing_slot_domain",
                )
                .with_evidence(ValidationEvidence::new(
                    "slot_id",
                    slot.id().get().to_string(),
                )),
            );
        }
    }

    let mut seen = Vec::new();
    for (index, domain) in query.domains().iter().enumerate() {
        if query.template().slot(domain.slot_id()).is_none() {
            report.push(
                invalid_build_query(
                    "build.domains",
                    "slot domain references a slot that is not in the template",
                    "unknown_slot_domain",
                )
                .with_location(EvidenceLocation::with_index("build.domains", index))
                .with_evidence(ValidationEvidence::new(
                    "slot_id",
                    domain.slot_id().get().to_string(),
                )),
            );
        }
        if domain.is_empty() {
            report.push(
                invalid_build_query(
                    "build.domains",
                    "slot domain must allow at least one piece",
                    "empty_slot_domain",
                )
                .with_location(EvidenceLocation::with_index("build.domains", index))
                .with_evidence(ValidationEvidence::new(
                    "slot_id",
                    domain.slot_id().get().to_string(),
                )),
            );
        }
        if let Some(slot) = query.template().slot(domain.slot_id()) {
            for piece in domain.pieces() {
                if !slot.allowed_pieces().contains(piece) {
                    report.push(
                        invalid_build_query(
                            "build.domains",
                            "slot domain contains a piece outside the template slot domain",
                            "domain_piece_not_allowed_by_template",
                        )
                        .with_location(EvidenceLocation::with_index("build.domains", index))
                        .with_evidence(ValidationEvidence::new(
                            "slot_id",
                            domain.slot_id().get().to_string(),
                        ))
                        .with_evidence(ValidationEvidence::new(
                            "piece",
                            piece.as_ascii().to_string(),
                        )),
                    );
                }
            }
        }
        for (piece_index, piece) in domain.pieces().iter().enumerate() {
            if domain.pieces()[..piece_index].contains(piece) {
                report.push(
                    invalid_build_query(
                        "build.domains",
                        "slot domain contains duplicate pieces",
                        "duplicate_domain_piece",
                    )
                    .with_location(EvidenceLocation::with_index("build.domains", index))
                    .with_evidence(ValidationEvidence::new(
                        "slot_id",
                        domain.slot_id().get().to_string(),
                    ))
                    .with_evidence(ValidationEvidence::new(
                        "piece",
                        piece.as_ascii().to_string(),
                    )),
                );
            }
        }
        if seen.contains(&domain.slot_id()) {
            report.push(
                invalid_build_query(
                    "build.domains",
                    "build query contains duplicate domains for the same slot",
                    "duplicate_slot_domain",
                )
                .with_location(EvidenceLocation::with_index("build.domains", index))
                .with_evidence(ValidationEvidence::new(
                    "slot_id",
                    domain.slot_id().get().to_string(),
                )),
            );
        } else {
            seen.push(domain.slot_id());
        }
    }
}

pub(crate) fn domain_for_slot(domains: &[SlotDomain], slot_id: BuildSlotId) -> Option<&SlotDomain> {
    domains.iter().find(|domain| domain.slot_id() == slot_id)
}
