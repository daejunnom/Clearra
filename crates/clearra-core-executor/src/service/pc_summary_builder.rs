use clearra_problem::SearchProblem;

use crate::{buildup::BuildUpRunResult, packing::PackingRunResult, service::field};

use super::pc_tiling_materialization::{
    PcTilingMaterialization, ACTUAL_TILING_SOLUTION_SET_CONTRACT,
};

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

pub(crate) fn tiling_result_count_fields(
    problem: &SearchProblem,
    packing: &PackingRunResult,
    materialization: &PcTilingMaterialization,
) -> Vec<(String, String)> {
    let normalized_solution_count = materialization.normalized_solution_count();
    vec![
        field("search_output_policy", problem.output_policy().as_str()),
        field(
            "count_mode",
            if materialization.packing_source_raw_geometry() {
                "count-all-geometry-candidates"
            } else {
                "count-all-buildability-prefiltered-candidates"
            },
        ),
        field("count_requested", true),
        field("count_complete", materialization.complete()),
        field(
            "count_truncated_reason",
            materialization.incomplete_reason(),
        ),
        field(
            "packing_multiset_group_count",
            packing.multiset_group_count(),
        ),
        field("total_solution_count", normalized_solution_count),
        field("unique_solution_count", normalized_solution_count),
        field(
            "actual_solution_set_contract",
            ACTUAL_TILING_SOLUTION_SET_CONTRACT,
        ),
        field(
            "normalized_solution_key_algorithm",
            materialization.normalized_key_algorithm(),
        ),
        field(
            "normalized_solution_set_hash_algorithm",
            materialization.normalized_hash_algorithm(),
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
            materialization.normalized_hash(),
        ),
        field(
            "actual_normalized_solution_set_hash",
            materialization.normalized_hash(),
        ),
        field("solution_count_calculated", true),
        field("solution_set_materialized", true),
        field(
            "solution_keys_materialized_count",
            materialization.initial_page_count(),
        ),
        field(
            "solution_keys_complete",
            materialization.complete() && materialization.initial_page_covers_family(),
        ),
        field(
            "solution_page_available",
            materialization.solution_page_available(),
        ),
        field("tiling_family_complete", materialization.complete()),
        field(
            "tiling_family_incomplete_reason",
            materialization.incomplete_reason(),
        ),
        field(
            "tiling_initial_page_count",
            materialization.initial_page_count(),
        ),
        field("tiling_initial_page_complete", true),
        field(
            "tiling_initial_page_covers_family",
            materialization.initial_page_covers_family(),
        ),
        field("objective_solution_traces", 0),
        field("objective_unique_solution_traces", 0),
        field("solution_trace_mode", "not-produced-tiling"),
        field("retained_trace_count", 0),
        field("trace_retention_truncated", false),
        field("trace_retention_reason", "not-produced-tiling"),
        field("sample_trace_available", false),
        field("solution_trace_count", 0),
        field("unique_solution_trace_count", 0),
        field("solution_trace_available", false),
        field("trace_steps", 0),
        field("trace_available", false),
        field("placed_pieces", 0),
    ]
}

/// Backend-neutral closed field adapter for the canonical `pc.tiling`
/// family. Native materialization and WASM search obtain their counts/stores
/// differently; this adapter keeps the externally validated family semantics
/// in one vocabulary without granting generic `ObjectivePolicy::tiling()` the
/// typed product identity.
pub(crate) fn canonical_tiling_family_result_fields(
    problem: &SearchProblem,
    normalized_solution_count: usize,
    normalized_hash: &str,
    initial_page_count: usize,
    complete: bool,
    incomplete_reason: &str,
    memory_admission_accounted: bool,
) -> Vec<(String, String)> {
    let initial_page_covers_family = initial_page_count == normalized_solution_count;
    vec![
        field("problem_preset", problem.preset().as_str()),
        field("compiled_goal", problem.goal().as_str()),
        field("search_output_policy", problem.output_policy().as_str()),
        field(
            "actual_solution_set_contract",
            ACTUAL_TILING_SOLUTION_SET_CONTRACT,
        ),
        field("packing_source_raw_geometry", true),
        field("packing_source_buildability_preverified", false),
        field("tiling_objective_canonical", true),
        field(
            "tiling_materialization_memory_admission_accounted",
            memory_admission_accounted,
        ),
        field("tiling_materialization_complete", complete),
        field("tiling_materialization_incomplete_reason", incomplete_reason),
        field("tiling_family_complete", complete),
        field("tiling_family_incomplete_reason", incomplete_reason),
        field("tiling_initial_page_count", initial_page_count),
        field("tiling_initial_page_complete", true),
        field(
            "tiling_initial_page_covers_family",
            initial_page_covers_family,
        ),
        field("count_complete", complete),
        field("count_truncated_reason", incomplete_reason),
        field("total_solution_count", normalized_solution_count),
        field("unique_solution_count", normalized_solution_count),
        field(
            "normalized_unique_solution_count",
            normalized_solution_count,
        ),
        field(
            "actual_normalized_unique_solution_count",
            normalized_solution_count,
        ),
        field(
            "normalized_solution_key_algorithm",
            clearra_core_domain::solution::normalized_tiling_solution::NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM,
        ),
        field(
            "normalized_solution_set_hash_algorithm",
            clearra_core_domain::solution::normalized_tiling_solution::NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
        ),
        field("normalized_solution_set_hash", normalized_hash),
        field("actual_normalized_solution_set_hash", normalized_hash),
        field("solution_count_calculated", true),
        field("solution_set_materialized", true),
        field("solution_keys_materialized_count", initial_page_count),
        field(
            "solution_keys_complete",
            complete && initial_page_covers_family,
        ),
        field(
            "solution_page_available",
            initial_page_count < normalized_solution_count,
        ),
        field("buildup_executed", false),
        field("additional_buildup_executed", false),
        field("buildability_verified", false),
        field("coverage_calculated", false),
        field("probability_calculated", false),
        field("solution_probabilities_requested", false),
    ]
}
