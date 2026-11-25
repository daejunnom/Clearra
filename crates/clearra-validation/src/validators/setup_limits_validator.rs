use clearra_setup_search::query::SetupLimits;

use crate::diagnostic::diagnostic_report::DiagnosticReport;

use super::setup_query_diagnostic_builder::invalid_setup_query;

pub(super) fn validate_limits(limits: SetupLimits, report: &mut DiagnosticReport) {
    // Generation limits are caller budgets, not capability ceilings. Zero
    // means complete generation; larger explicit budgets remain valid.
    if limits.post_pc_retained_trace_limit() == 0 {
        report.push(invalid_setup_query(
            "setup.limits",
            "setup witness retention must be greater than zero",
            "zero_post_pc_retained_trace_limit",
        ));
    }
}
