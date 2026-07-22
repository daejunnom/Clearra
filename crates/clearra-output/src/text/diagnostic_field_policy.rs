use super::human_summary_field_policy::HumanSummaryFieldPolicy;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DiagnosticFieldPolicy;

impl DiagnosticFieldPolicy {
    pub fn include_field(kind: &str, key: &str) -> bool {
        HumanSummaryFieldPolicy::include_field(kind, key)
            || key.contains("diagnostic")
            || key.contains("warning")
            || key.contains("error")
            || key.contains("evidence")
            || key == "suggested_next_step"
            || key.contains("fallback")
            || key.contains("unsupported")
            || key.contains("reason")
            || key.contains("blocked")
            || key.contains("authenticode")
            || key.contains("zone_identifier")
            || key.contains("trust_state")
            || key.contains("truncation")
            || key.contains("complete")
    }
}
