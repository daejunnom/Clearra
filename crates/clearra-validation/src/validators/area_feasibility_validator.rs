use clearra_geometry::area::{AreaMultisetFeasibility, AreaScopeDescriptor};

use crate::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        diagnostic_report::DiagnosticReport, suggested_next_step::SuggestedNextStep,
    },
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
};

pub struct AreaFeasibilityValidator;

impl AreaFeasibilityValidator {
    pub fn validate_components(
        scope: &AreaScopeDescriptor,
        component_areas: &[usize],
        feasibility: &AreaMultisetFeasibility,
    ) -> DiagnosticReport {
        let mut report = DiagnosticReport::new();

        for component_area in component_areas {
            if !feasibility.can_fill_exactly(*component_area) {
                report.push(area_infeasible_diagnostic(
                    scope.scope_kind(),
                    *component_area,
                    feasibility,
                ));
                return report;
            }
        }

        report.push(area_necessary_condition_passed_diagnostic(
            scope.scope_kind(),
            component_areas,
            feasibility,
        ));
        report
    }
}

fn area_infeasible_diagnostic(
    scope_kind: &'static str,
    component_area: usize,
    feasibility: &AreaMultisetFeasibility,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::EAreaInfeasible,
        "area component cannot be composed from the active piece area multiset",
    )
    .with_location(EvidenceLocation::new("compile.area_pruner"))
    .with_evidence(ValidationEvidence::new("scope_kind", scope_kind))
    .with_evidence(ValidationEvidence::new(
        "component_area",
        component_area.to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "active_piece_area_multiset",
        feasibility
            .active_piece_area_multiset()
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(","),
    ))
    .with_evidence(ValidationEvidence::new(
        "area_decomposition_role",
        "necessary-condition-not-solver",
    ))
    .with_suggested_next_step(SuggestedNextStep::new(
        "Adjust the target area scope or active piece area multiset before running search.",
    ))
}

fn area_necessary_condition_passed_diagnostic(
    scope_kind: &'static str,
    component_areas: &[usize],
    feasibility: &AreaMultisetFeasibility,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::IAreaNecessaryConditionPassed,
        "area components passed necessary-condition pruning; search is still required",
    )
    .with_location(EvidenceLocation::new("compile.area_pruner"))
    .with_evidence(ValidationEvidence::new("scope_kind", scope_kind))
    .with_evidence(ValidationEvidence::new(
        "component_areas",
        component_areas
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(","),
    ))
    .with_evidence(ValidationEvidence::new(
        "active_piece_area_multiset",
        feasibility
            .active_piece_area_multiset()
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(","),
    ))
    .with_evidence(ValidationEvidence::new(
        "area_feasible_is_solution_found",
        "false",
    ))
}

#[cfg(test)]
#[path = "area_feasibility_validator_tests.rs"]
mod tests;
