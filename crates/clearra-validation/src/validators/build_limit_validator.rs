use clearra_build_coverage::query::build_coverage_query::BuildCoverageQuery;

use crate::{
    diagnostic::diagnostic_report::DiagnosticReport,
    evidence::validation_evidence::ValidationEvidence,
    validators::build_query_validator::invalid_build_query,
};

pub(crate) fn validate_limits(query: &BuildCoverageQuery, report: &mut DiagnosticReport) {
    if query.pattern_count() == 0 {
        report.push(invalid_build_query(
            "build.patterns",
            "build coverage pattern count must be greater than zero",
            "zero_pattern_count",
        ));
    }

    if query.limits().max_assignments() == 0 {
        report.push(invalid_build_query(
            "build.limits",
            "build coverage assignment limit must be greater than zero",
            "zero_build_limit",
        ));
    }

    if query.pattern_count() > query.limits().max_patterns() {
        report.push(
            invalid_build_query(
                "build.patterns",
                "build coverage pattern count exceeds the configured limit",
                "pattern_limit_exceeded",
            )
            .with_evidence(ValidationEvidence::new(
                "pattern_count",
                query.pattern_count().to_string(),
            ))
            .with_evidence(ValidationEvidence::new(
                "max_patterns",
                query.limits().max_patterns().to_string(),
            )),
        );
    }
}
