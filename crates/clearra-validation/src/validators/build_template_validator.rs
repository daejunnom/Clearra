use clearra_build_coverage::{
    query::build_coverage_query::BuildCoverageQuery, template::build_slot::BuildSlotId,
};

use crate::{
    diagnostic::diagnostic_report::DiagnosticReport,
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
    validators::{
        build_query_validator::invalid_build_query,
        build_slot_domain_validator::validate_template_slot_domain,
        build_slot_geometry_validator::validate_slot_geometry,
        build_slot_order_validator::validate_slot_order_constraint,
    },
};

pub(crate) fn validate_template(query: &BuildCoverageQuery, report: &mut DiagnosticReport) {
    if query.template().slots().is_empty() {
        report.push(invalid_build_query(
            "build.template.slots",
            "build template must contain at least one slot",
            "empty_template",
        ));
        return;
    }

    let mut seen = Vec::new();
    for (index, slot) in query.template().slots().iter().enumerate() {
        validate_slot_geometry(query, slot, index, report);
        validate_template_slot_domain(slot, index, report);
        validate_slot_order_constraint(query, slot, index, report);
        validate_duplicate_slot_id(slot.id(), index, &mut seen, report);
    }
}

fn validate_duplicate_slot_id(
    slot_id: BuildSlotId,
    index: usize,
    seen: &mut Vec<BuildSlotId>,
    report: &mut DiagnosticReport,
) {
    if seen.contains(&slot_id) {
        report.push(
            invalid_build_query(
                "build.template.slots",
                "build template contains duplicate slot ids",
                "duplicate_slot_id",
            )
            .with_location(EvidenceLocation::with_index("build.template.slots", index))
            .with_evidence(ValidationEvidence::new(
                "slot_id",
                slot_id.get().to_string(),
            )),
        );
    } else {
        seen.push(slot_id);
    }
}
