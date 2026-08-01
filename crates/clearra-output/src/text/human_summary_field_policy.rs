#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct HumanSummaryFieldPolicy;

impl HumanSummaryFieldPolicy {
    pub fn include_field(kind: &str, key: &str) -> bool {
        if is_common_summary_field(key) || is_common_result_field(key) {
            return true;
        }

        if is_probability_query_field(kind, key) {
            return true;
        }

        match kind {
            "pc" | "pc-scenario" | "path" | "percent" | "continue" => {
                is_pc_family_summary_field(key)
            }
            "setup" => is_setup_summary_field(key),
            "cover" => is_cover_summary_field(key),
            _ => true,
        }
    }
}

fn is_common_summary_field(key: &str) -> bool {
    matches!(
        key,
        "status"
            | "problem_preset"
            | "preset"
            | "lines"
            | "queue_len"
            | "queue_mode"
            | "hold_enabled"
            | "rule_profile"
            | "solver_backend"
            | "unsupported_reason"
            | "warning_count"
            | "diagnostic_count"
    )
}

fn is_common_result_field(key: &str) -> bool {
    matches!(
        key,
        "solution_found"
            | "total_solution_count"
            | "unique_solution_count"
            | "coverage_probability"
            | "covered_pattern_count"
            | "count_complete"
            | "retained_trace_count"
            | "trace_retention_truncated"
            | "trace_retention_reason"
            | "next_pc_available"
            | "continuation_token_available"
            | "continue_hint"
            | "continuation_kind"
    )
}

fn is_probability_query_field(kind: &str, key: &str) -> bool {
    matches!(kind, "percent" | "setup")
        && (matches!(
            key,
            "materialized_probability_mass"
                | "probability_complete"
                | "renormalized"
                | "truncation_reason"
                | "result_mode"
                | "failed_queue_contract"
                | "failed_queue_probability"
                | "total_pattern_count"
                | "failed_pattern_count"
                | "failed_pattern_scope"
                | "failed_pattern_count_complete"
                | "failed_pattern_limit"
                | "failed_pattern_examples_materialized"
                | "failed_pattern_examples_truncated"
        ) || key.starts_with("failed_pattern_"))
}

fn is_pc_family_summary_field(key: &str) -> bool {
    matches!(key, "objective" | "interactive_prompt")
}

fn is_setup_summary_field(key: &str) -> bool {
    matches!(
        key,
        "shape_family_id"
            | "build_variant_count"
            | "tiling_variant_count"
            | "post_pc_solution_count"
            | "score_basis"
    )
}

fn is_cover_summary_field(key: &str) -> bool {
    matches!(
        key,
        "template" | "template_id" | "action" | "exported" | "coverage_row_source"
    )
}

#[cfg(test)]
mod tests {
    use super::HumanSummaryFieldPolicy;

    #[test]
    fn percent_human_output_keeps_failed_queue_fields() {
        for key in [
            "result_mode",
            "failed_queue_probability",
            "failed_pattern_count",
            "failed_pattern_0",
        ] {
            assert!(
                HumanSummaryFieldPolicy::include_field("percent", key),
                "{key}"
            );
        }
        assert!(!HumanSummaryFieldPolicy::include_field(
            "pc",
            "failed_pattern_0"
        ));
    }
}
