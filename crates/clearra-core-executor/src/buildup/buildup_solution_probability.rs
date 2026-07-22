use std::collections::BTreeMap;

use clearra_core_domain::solution::normalized_tiling_solution::StandardBoard64TilingIdentity;
use clearra_core_ffi::PackingCandidateIdentityError;
use clearra_coverage::{pattern::pattern_bitset::PatternBitSet, row::coverage_row::CoverageRow};
use clearra_problem::SearchProblem;

use crate::{
    buildup::{
        buildup_error::BuildUpRunnerError,
        buildup_solution_set_contract::BuildUpSolutionSetContract,
    },
    packing::PackingRunResult,
    solution_probability::{
        covers_all_identities, probability_reports, SolutionCoverage, SolutionProbabilityReport,
    },
};

pub(super) struct BuildUpSolutionProbabilityBatch {
    pub coverage: Vec<SolutionCoverage>,
    pub reports: Vec<SolutionProbabilityReport>,
    pub complete: bool,
}

pub(super) fn collect_solution_probabilities(
    problem: &SearchProblem,
    packing: &PackingRunResult,
    coverage_rows: &[CoverageRow],
    solution_set: &BuildUpSolutionSetContract,
    inputs_complete: bool,
) -> Result<BuildUpSolutionProbabilityBatch, BuildUpRunnerError> {
    if !problem.solution_probability_policy().requested() {
        return Ok(BuildUpSolutionProbabilityBatch {
            coverage: Vec::new(),
            reports: Vec::new(),
            complete: true,
        });
    }

    let Some(weights) = problem.piece_source().materialized_pattern_weights() else {
        return Ok(BuildUpSolutionProbabilityBatch {
            coverage: Vec::new(),
            reports: Vec::new(),
            complete: false,
        });
    };

    let mut identities_by_candidate = BTreeMap::new();
    for candidate_index in 0..packing.candidate_count() {
        let candidate = packing
            .candidate_view_at(candidate_index)
            .ok_or(BuildUpRunnerError::PackingCandidateUnavailable { candidate_index })?;
        identities_by_candidate.insert(
            candidate.candidate_id(),
            candidate
                .standard_board64_tiling_identity(problem.initial_board().occupied_mask())
                .map_err(map_identity_error)?,
        );
    }

    let pattern_count = weights.len();
    let mut by_identity = BTreeMap::<StandardBoard64TilingIdentity, PatternBitSet>::new();
    for row in coverage_rows {
        let identity = identities_by_candidate
            .get(&row.candidate_id())
            .copied()
            .ok_or(
                BuildUpRunnerError::SolutionProbabilityCandidateUnavailable {
                    candidate_id: row.candidate_id(),
                },
            )?;
        let entry = by_identity
            .entry(identity)
            .or_insert_with(|| PatternBitSet::new(pattern_count));
        entry
            .union_with(row.coverage_bits())
            .map_err(BuildUpRunnerError::Pattern)?;
    }

    let coverage = by_identity
        .into_iter()
        .map(|(identity, bits)| SolutionCoverage::new(identity, bits))
        .collect::<Vec<_>>();
    let complete = inputs_complete
        && problem.piece_source().complete()
        && covers_all_identities(solution_set.identities(), &coverage);
    let reports = probability_reports(solution_set.identities(), &coverage, weights, complete);
    Ok(BuildUpSolutionProbabilityBatch {
        coverage,
        reports,
        complete,
    })
}

fn map_identity_error(error: PackingCandidateIdentityError) -> BuildUpRunnerError {
    match error {
        PackingCandidateIdentityError::UnknownPieceCode(code) => {
            BuildUpRunnerError::UnknownPackingPieceCode { code }
        }
        PackingCandidateIdentityError::InvalidTiling(error) => {
            BuildUpRunnerError::NormalizedTiling(error)
        }
    }
}
