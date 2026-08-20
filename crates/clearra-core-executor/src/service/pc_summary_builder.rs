use clearra_problem::SearchProblem;

use crate::{buildup::BuildUpRunResult, packing::PackingRunResult, service::field};

pub(crate) fn result_count_fields(
    problem: &SearchProblem,
    packing: &PackingRunResult,
    buildup: &BuildUpRunResult,
    supply_complete: bool,
) -> Vec<(String, String)> {
    let count_complete = supply_complete && packing.count_complete() && buildup.count_complete();
    let count_truncated_reason = if let Some(reason) = packing.truncation_reason() {
        reason.as_str()
    } else if !supply_complete {
        "observed_universe_truncated"
    } else {
        buildup.count_truncated_reason()
    };
    let normalized_solution_count = buildup.normalized_unique_solution_count();
    vec![
        field("search_output_policy", problem.output_policy().as_str()),
        field("count_mode", "count-all"),
        field("count_requested", "true"),
        field("count_complete", count_complete),
        field("count_truncated_reason", count_truncated_reason),
        field(
            "packing_multiset_group_count",
            packing.multiset_group_count(),
        ),
        field("total_solution_count", buildup.total_solution_count()),
        field("unique_solution_count", buildup.unique_solution_count()),
        field(
            "actual_solution_set_contract",
            buildup.actual_solution_set_contract(),
        ),
        field(
            "normalized_solution_key_algorithm",
            buildup.normalized_solution_key_algorithm(),
        ),
        field(
            "normalized_solution_set_hash_algorithm",
            buildup.normalized_solution_set_hash_algorithm(),
        ),
        field(
            "normalized_unique_solution_count",
            normalized_solution_count,
        ),
        field(
            "actual_normalized_unique_solution_count",
            normalized_solution_count,
        ),
        field(
            "normalized_solution_set_hash",
            buildup.normalized_solution_set_hash(),
        ),
        field(
            "actual_normalized_solution_set_hash",
            buildup.normalized_solution_set_hash(),
        ),
        field("solution_count_calculated", true),
        field("solution_set_materialized", true),
        field(
            "solution_keys_materialized_count",
            normalized_solution_count,
        ),
        field("solution_keys_complete", count_complete),
        field("solution_page_available", false),
        field("objective_solution_traces", buildup.retained_trace_count()),
        field(
            "objective_unique_solution_traces",
            buildup.retained_trace_count(),
        ),
        field("solution_trace_mode", "retained-traces"),
        field("retained_trace_count", buildup.retained_trace_count()),
        field(
            "trace_retention_truncated",
            buildup.trace_retention_truncated(),
        ),
        field("trace_retention_reason", buildup.trace_retention_reason()),
        field("sample_trace_available", buildup.retained_trace_count() > 0),
        field("solution_trace_count", buildup.retained_trace_count()),
        field(
            "unique_solution_trace_count",
            buildup.retained_trace_count(),
        ),
        field(
            "solution_trace_available",
            buildup.retained_trace_count() > 0,
        ),
        field("trace_steps", buildup.path_steps().len()),
        field("trace_available", !buildup.path_steps().is_empty()),
        field("placed_pieces", buildup.path_steps().len()),
    ]
}
