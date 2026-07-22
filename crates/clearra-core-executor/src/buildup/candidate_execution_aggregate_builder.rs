use std::collections::{BTreeMap, BTreeSet};

use clearra_core_ffi::CBuildVariantView;
use clearra_coverage::row::coverage_row::CoverageRow;

use crate::buildup::{
    buildup_error::BuildUpRunnerError,
    buildup_trace_retention::{stable_key_for_coverage_row, trace_key_for_build_variant},
    candidate_execution_aggregate::CandidateExecutionAggregate,
};

pub(crate) fn aggregate_candidate_executions(
    variants: &[CBuildVariantView],
    rows: &[CoverageRow],
) -> Result<Vec<CandidateExecutionAggregate>, BuildUpRunnerError> {
    let mut variants_by_candidate = BTreeMap::<u64, Vec<CBuildVariantView>>::new();
    for variant in variants {
        variants_by_candidate
            .entry(variant.candidate_id())
            .or_default()
            .push(variant.clone());
    }

    let mut seen_candidates = BTreeSet::new();
    rows.iter()
        .map(|row| {
            if !seen_candidates.insert(row.candidate_id()) {
                return Err(BuildUpRunnerError::DuplicateObjectiveCoverageCandidate {
                    candidate_id: row.candidate_id(),
                });
            }
            let mut execution_variants = variants_by_candidate
                .remove(&row.candidate_id())
                .unwrap_or_default();
            execution_variants
                .sort_by_key(|variant| (variant.build_variant_id(), variant.coverage_pattern_id()));
            let representative_trace = execution_variants
                .first()
                .map(|variant| trace_key_for_build_variant(variant, row.candidate_id()))
                .or_else(|| Some(stable_key_for_coverage_row(row)));
            Ok(CandidateExecutionAggregate::new(
                row.clone(),
                execution_variants,
                representative_trace,
            ))
        })
        .collect()
}
