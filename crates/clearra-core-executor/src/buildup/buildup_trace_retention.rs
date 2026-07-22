use clearra_core_ffi::CBuildVariantView;
#[cfg(test)]
use clearra_coverage::row::coverage_row::CoverageRow;
use clearra_problem::SearchProblem;

use crate::packing::scenario_packing_witness::ScenarioPackingWitness;

pub(crate) fn retained_trace_count(
    problem: &SearchProblem,
    witness: ScenarioPackingWitness,
    materialized_trace_count: usize,
) -> usize {
    if !witness.solution_found {
        return 0;
    }
    witness
        .total_solution_count
        .min(materialized_trace_count)
        .min(problem.core_query().retained_trace_limit())
}

pub(crate) fn trace_key_for_build_variant(
    variant: &CBuildVariantView,
    candidate_id: u64,
) -> String {
    let canonical_candidate_id = if variant.candidate_id() == 0 {
        candidate_id
    } else {
        variant.candidate_id()
    };
    format!(
        "bvk2:{canonical_candidate_id:016x}:{:08x}:{:016x}",
        variant.coverage_pattern_id(),
        variant.build_variant_id()
    )
}

#[cfg(test)]
pub(crate) fn stable_key_for_coverage_row(row: &CoverageRow) -> String {
    let candidate_id = row.candidate_id();
    format!("crk1:{candidate_id:016x}")
}

pub(crate) fn format_probability(value: f64) -> String {
    if value == 0.0 {
        "0.0".to_owned()
    } else if value == 1.0 {
        "1.0".to_owned()
    } else {
        let formatted = format!("{value:.12}");
        formatted
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_owned()
    }
}
