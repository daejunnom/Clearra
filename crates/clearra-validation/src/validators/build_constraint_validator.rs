use clearra_build_coverage::{
    query::build_coverage_query::BuildCoverageQuery, template::build_slot::BuildSlotId,
};
use clearra_core_domain::piece::piece_kind::PieceKind;

use crate::{
    diagnostic::diagnostic_report::DiagnosticReport,
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
    validators::{
        build_query_validator::invalid_build_query, build_slot_domain_validator::domain_for_slot,
    },
};

pub(crate) fn validate_constraints(query: &BuildCoverageQuery, report: &mut DiagnosticReport) {
    let mut required_by_slot: Vec<(BuildSlotId, PieceKind)> = Vec::new();
    for (index, constraint) in query.constraints().iter().enumerate() {
        if query.template().slot(constraint.slot_id()).is_none() {
            report.push(
                invalid_build_query(
                    "build.constraints",
                    "slot constraint references a slot that is not in the template",
                    "unknown_slot_constraint",
                )
                .with_location(EvidenceLocation::with_index("build.constraints", index))
                .with_evidence(ValidationEvidence::new(
                    "slot_id",
                    constraint.slot_id().get().to_string(),
                )),
            );
            continue;
        }

        let Some(required_piece) = constraint.required_piece() else {
            continue;
        };

        if domain_for_slot(query.domains(), constraint.slot_id())
            .is_some_and(|domain| !domain.pieces().contains(&required_piece))
        {
            report.push(
                invalid_build_query(
                    "build.constraints",
                    "required slot piece must be inside the slot domain",
                    "required_piece_not_in_domain",
                )
                .with_location(EvidenceLocation::with_index("build.constraints", index))
                .with_evidence(ValidationEvidence::new(
                    "slot_id",
                    constraint.slot_id().get().to_string(),
                ))
                .with_evidence(ValidationEvidence::new(
                    "piece",
                    required_piece.as_ascii().to_string(),
                )),
            );
        }

        if let Some((_, previous_piece)) = required_by_slot
            .iter()
            .find(|(slot_id, _)| *slot_id == constraint.slot_id())
        {
            if *previous_piece != required_piece {
                report.push(
                    invalid_build_query(
                        "build.constraints",
                        "slot has conflicting required piece constraints",
                        "required_piece_conflict",
                    )
                    .with_location(EvidenceLocation::with_index("build.constraints", index))
                    .with_evidence(ValidationEvidence::new(
                        "slot_id",
                        constraint.slot_id().get().to_string(),
                    ))
                    .with_evidence(ValidationEvidence::new(
                        "first_piece",
                        previous_piece.as_ascii().to_string(),
                    ))
                    .with_evidence(ValidationEvidence::new(
                        "second_piece",
                        required_piece.as_ascii().to_string(),
                    )),
                );
            }
        } else {
            required_by_slot.push((constraint.slot_id(), required_piece));
        }
    }
}
