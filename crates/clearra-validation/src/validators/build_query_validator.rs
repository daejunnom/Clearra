use clearra_build_coverage::query::build_coverage_query::BuildCoverageQuery;

use crate::{
    diagnostic::{
        diagnostic::Diagnostic, diagnostic_code::DiagnosticCode,
        diagnostic_report::DiagnosticReport, suggested_next_step::SuggestedNextStep,
    },
    evidence::{evidence_location::EvidenceLocation, validation_evidence::ValidationEvidence},
    validators::{
        build_assignment_feasibility_validator::validate_impossible_assignment,
        build_constraint_validator::validate_constraints, build_limit_validator::validate_limits,
        build_slot_domain_validator::validate_domains, build_template_validator::validate_template,
    },
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BuildQueryValidator;

impl BuildQueryValidator {
    pub fn validate(query: &BuildCoverageQuery) -> DiagnosticReport {
        let mut report = DiagnosticReport::new();

        validate_template(query, &mut report);
        validate_domains(query, &mut report);
        validate_constraints(query, &mut report);
        validate_impossible_assignment(query, &mut report);
        validate_limits(query, &mut report);

        if !report.has_errors() {
            report.push(build_query_supported_diagnostic(query));
        }

        report
    }
}

pub fn validate_build_coverage_query(query: &BuildCoverageQuery) -> DiagnosticReport {
    BuildQueryValidator::validate(query)
}

fn build_query_supported_diagnostic(query: &BuildCoverageQuery) -> Diagnostic {
    Diagnostic::new(
        DiagnosticCode::IBuildQueryMvpSupported,
        "build coverage query is valid for the MVP1 template/domain/CSP path",
    )
    .with_location(EvidenceLocation::new("build.query"))
    .with_evidence(ValidationEvidence::new(
        "slot_count",
        query.template().slots().len().to_string(),
    ))
    .with_evidence(ValidationEvidence::new(
        "domain_count",
        query.domains().len().to_string(),
    ))
}

pub(crate) fn invalid_build_query(
    location: &'static str,
    message: &'static str,
    reason: &'static str,
) -> Diagnostic {
    Diagnostic::new(DiagnosticCode::EBuildQueryInvalid, message)
        .with_location(EvidenceLocation::new(location))
        .with_evidence(ValidationEvidence::new("reason", reason))
        .with_suggested_next_step(SuggestedNextStep::new(
            "Fix the build template, domains, constraints, or limits before running build coverage.",
        ))
}

#[cfg(test)]
#[path = "build_query_validator_tests.rs"]
mod tests;
