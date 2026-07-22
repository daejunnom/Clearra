use clearra_build_coverage::{
    assignment::assignment_csp::{AssignmentCsp, AssignmentCspLimits},
    domain::slot_constraint::SlotConstraint,
    query::build_coverage_query::BuildCoverageQuery,
};

use crate::{
    diagnostic::diagnostic_report::DiagnosticReport,
    evidence::validation_evidence::ValidationEvidence,
    validators::{
        build_query_validator::invalid_build_query, build_slot_domain_validator::domain_for_slot,
    },
};

pub(crate) fn validate_impossible_assignment(
    query: &BuildCoverageQuery,
    report: &mut DiagnosticReport,
) {
    if !assignment_contract_is_well_formed(query) {
        return;
    }

    let csp = AssignmentCsp::new(
        query.domains().to_vec(),
        effective_constraints(query),
        AssignmentCspLimits::new(1),
    );
    if csp.solve().is_empty() {
        report.push(
            invalid_build_query(
                "build.assignments",
                "build query has no feasible slot assignment",
                "impossible_assignment",
            )
            .with_evidence(ValidationEvidence::new(
                "slot_count",
                query.template().slots().len().to_string(),
            ))
            .with_evidence(ValidationEvidence::new(
                "domain_count",
                query.domains().len().to_string(),
            )),
        );
    }
}

fn assignment_contract_is_well_formed(query: &BuildCoverageQuery) -> bool {
    if query.template().slots().is_empty() {
        return false;
    }

    query.template().slots().iter().all(|slot| {
        domain_for_slot(query.domains(), slot.id()).is_some_and(|domain| {
            !domain.is_empty()
                && domain
                    .pieces()
                    .iter()
                    .all(|piece| slot.allowed_pieces().contains(piece))
        })
    }) && query
        .domains()
        .iter()
        .all(|domain| query.template().slot(domain.slot_id()).is_some())
        && query.constraints().iter().all(|constraint| {
            query.template().slot(constraint.slot_id()).is_some()
                && constraint.required_piece().is_none_or(|piece| {
                    domain_for_slot(query.domains(), constraint.slot_id())
                        .is_some_and(|domain| domain.pieces().contains(&piece))
                })
        })
}

fn effective_constraints(query: &BuildCoverageQuery) -> Vec<SlotConstraint> {
    let mut constraints = query.constraints().to_vec();
    for slot in query.template().slots() {
        if let Some(required_piece) = slot.required_piece() {
            constraints.push(SlotConstraint::required(slot.id(), required_piece));
        }
    }
    constraints
}
