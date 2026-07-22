use clearra_build_coverage::{
    query::build_coverage_query::BuildCoverageQuery, template::build_slot::BuildSlot,
};

use crate::{
    diagnostic::diagnostic_report::DiagnosticReport,
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
    validators::build_query_validator::invalid_build_query,
};

pub(crate) fn validate_slot_geometry(
    query: &BuildCoverageQuery,
    slot: &BuildSlot,
    index: usize,
    report: &mut DiagnosticReport,
) {
    if slot.cells().is_empty() {
        report.push(
            invalid_build_query(
                "build.template.slots",
                "build template slot must contain at least one cell",
                "empty_slot_cells",
            )
            .with_location(EvidenceLocation::with_index("build.template.slots", index))
            .with_evidence(ValidationEvidence::new(
                "slot_id",
                slot.id().get().to_string(),
            )),
        );
        return;
    }

    let mut seen_cells = Vec::new();
    for cell in slot.cells() {
        if cell.x() >= query.template().board_size().width()
            || cell.y() >= query.template().board_size().height()
        {
            report.push(
                invalid_build_query(
                    "build.template.slots",
                    "build template slot cell is outside the template board",
                    "slot_cell_out_of_bounds",
                )
                .with_location(EvidenceLocation::with_index("build.template.slots", index))
                .with_evidence(ValidationEvidence::new(
                    "slot_id",
                    slot.id().get().to_string(),
                ))
                .with_evidence(ValidationEvidence::new("x", cell.x().to_string()))
                .with_evidence(ValidationEvidence::new("y", cell.y().to_string())),
            );
        }

        if seen_cells.contains(cell) {
            report.push(
                invalid_build_query(
                    "build.template.slots",
                    "build template slot contains duplicate cells",
                    "duplicate_slot_cell",
                )
                .with_location(EvidenceLocation::with_index("build.template.slots", index))
                .with_evidence(ValidationEvidence::new(
                    "slot_id",
                    slot.id().get().to_string(),
                ))
                .with_evidence(ValidationEvidence::new("x", cell.x().to_string()))
                .with_evidence(ValidationEvidence::new("y", cell.y().to_string())),
            );
        } else {
            seen_cells.push(*cell);
        }
    }

    for (other_index, other_slot) in query.template().slots().iter().enumerate() {
        if other_index >= index {
            continue;
        }
        for cell in slot.cells() {
            if other_slot.cells().contains(cell) {
                report.push(
                    invalid_build_query(
                        "build.template.slots",
                        "build template slots must not overlap cells",
                        "overlapping_slot_cells",
                    )
                    .with_location(EvidenceLocation::with_index("build.template.slots", index))
                    .with_evidence(ValidationEvidence::new(
                        "slot_id",
                        slot.id().get().to_string(),
                    ))
                    .with_evidence(ValidationEvidence::new(
                        "other_slot_id",
                        other_slot.id().get().to_string(),
                    ))
                    .with_evidence(ValidationEvidence::new("x", cell.x().to_string()))
                    .with_evidence(ValidationEvidence::new("y", cell.y().to_string())),
                );
            }
        }
    }
}
