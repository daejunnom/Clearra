use clearra_build_coverage::{
    query::build_coverage_query::BuildCoverageQuery,
    template::build_slot::{BuildSlot, SlotOrderConstraint},
};

use crate::{
    diagnostic::diagnostic_report::DiagnosticReport,
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
    validators::build_query_validator::invalid_build_query,
};

pub(crate) fn validate_slot_order_constraint(
    query: &BuildCoverageQuery,
    slot: &BuildSlot,
    index: usize,
    report: &mut DiagnosticReport,
) {
    match slot.order_constraint() {
        SlotOrderConstraint::Any => {}
        SlotOrderConstraint::Before(other) | SlotOrderConstraint::After(other) => {
            if other == slot.id() {
                report.push(
                    invalid_build_query(
                        "build.template.slots",
                        "build template slot order constraint must not reference itself",
                        "self_referential_slot_order",
                    )
                    .with_location(EvidenceLocation::with_index("build.template.slots", index))
                    .with_evidence(ValidationEvidence::new(
                        "slot_id",
                        slot.id().get().to_string(),
                    )),
                );
            } else if query.template().slot(other).is_none() {
                report.push(
                    invalid_build_query(
                        "build.template.slots",
                        "build template slot order constraint references an unknown slot",
                        "unknown_slot_order_reference",
                    )
                    .with_location(EvidenceLocation::with_index("build.template.slots", index))
                    .with_evidence(ValidationEvidence::new(
                        "slot_id",
                        slot.id().get().to_string(),
                    ))
                    .with_evidence(ValidationEvidence::new(
                        "referenced_slot_id",
                        other.get().to_string(),
                    )),
                );
            }
        }
    }
}
