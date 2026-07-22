use clearra_setup_search::query::SetupLimits;

use crate::{
    diagnostic::diagnostic_report::DiagnosticReport,
    evidence::validation_evidence::ValidationEvidence,
};

use super::setup_query_diagnostic_builder::invalid_setup_query;

pub(super) fn validate_limits(limits: SetupLimits, report: &mut DiagnosticReport) {
    let mvp_limits = SetupLimits::default();
    validate_limit(
        "max_shape_families",
        limits.max_shape_families(),
        mvp_limits.max_shape_families(),
        report,
    );
    validate_limit(
        "max_tiling_variants_per_family",
        limits.max_tiling_variants_per_family(),
        mvp_limits.max_tiling_variants_per_family(),
        report,
    );
    validate_limit(
        "max_build_variants_per_tiling",
        limits.max_build_variants_per_tiling(),
        mvp_limits.max_build_variants_per_tiling(),
        report,
    );
    validate_limit(
        "max_results",
        limits.max_results(),
        mvp_limits.max_results(),
        report,
    );
    validate_limit(
        "max_patterns",
        limits.max_patterns(),
        mvp_limits.max_patterns(),
        report,
    );
    validate_limit(
        "post_pc_retained_trace_limit",
        limits.post_pc_retained_trace_limit(),
        mvp_limits.post_pc_retained_trace_limit(),
        report,
    );
}

fn validate_limit(
    name: &'static str,
    actual: usize,
    maximum: usize,
    report: &mut DiagnosticReport,
) {
    if actual > maximum {
        report.push(
            invalid_setup_query(
                "setup.limits",
                "setup query limit exceeds the MVP1 default budget",
                "limit_exceeded",
            )
            .with_evidence(ValidationEvidence::new("limit", name))
            .with_evidence(ValidationEvidence::new("actual", actual.to_string()))
            .with_evidence(ValidationEvidence::new("maximum", maximum.to_string())),
        );
    }
}
