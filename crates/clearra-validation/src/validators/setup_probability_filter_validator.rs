use clearra_setup_search::query::SetupProbabilityFilter;

use crate::{
    diagnostic::diagnostic_report::DiagnosticReport,
    evidence::validation_evidence::ValidationEvidence,
};

use super::setup_query_diagnostic_builder::invalid_setup_query;

pub(super) fn validate_probability_filter(
    probability_filter: SetupProbabilityFilter,
    report: &mut DiagnosticReport,
) {
    if let (Some(minimum), Some(maximum)) = (
        probability_filter.min_probability(),
        probability_filter.max_probability(),
    ) {
        if minimum > maximum {
            report.push(
                invalid_setup_query(
                    "setup.probability_filter",
                    "minimum setup probability must not exceed maximum setup probability",
                    "probability_min_exceeds_max",
                )
                .with_evidence(ValidationEvidence::new(
                    "minimum",
                    minimum.get().to_string(),
                ))
                .with_evidence(ValidationEvidence::new(
                    "maximum",
                    maximum.get().to_string(),
                )),
            );
        }
    }
}
