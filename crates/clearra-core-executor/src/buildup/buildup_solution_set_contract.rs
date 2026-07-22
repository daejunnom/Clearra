use std::sync::atomic::{AtomicUsize, Ordering};

use clearra_core_domain::{
    execution_cancellation::ExecutionCancellationToken,
    solution::{
        normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities,
        NormalizedTilingSolutionKey, StandardBoard64TilingIdentity,
        NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM, NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM,
    },
};
use clearra_core_ffi::PackingCandidateIdentityError;
use clearra_problem::SearchProblem;

use crate::{
    buildup::{
        buildup_candidate_acceptance::BuildUpCandidateAcceptance, buildup_error::BuildUpRunnerError,
    },
    packing::PackingRunResult,
};

pub(crate) const ACTUAL_SOLUTION_SET_CONTRACT: &str = "normalized-tiling-set";
const SOLUTION_SET_CHUNK_SIZE: usize = 4_096;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BuildUpSolutionSetContract {
    identities: Vec<StandardBoard64TilingIdentity>,
    solution_set_hash: String,
}

pub(crate) struct BuildUpSolutionSetSummary {
    pub(crate) contract: BuildUpSolutionSetContract,
    pub(crate) accepted_candidate_count: usize,
}

impl BuildUpSolutionSetContract {
    pub(crate) fn from_packing_results(
        problem: &SearchProblem,
        packing: &PackingRunResult,
        candidate_acceptance: &BuildUpCandidateAcceptance,
        cancellation: &ExecutionCancellationToken,
    ) -> Result<BuildUpSolutionSetSummary, BuildUpRunnerError> {
        if packing.candidate_count() != candidate_acceptance.len() {
            return Err(BuildUpRunnerError::BuildUpResultCountMismatch {
                candidate_count: packing.candidate_count(),
                result_count: candidate_acceptance.len(),
            });
        }
        let worker_count = problem
            .backend_request()
            .workers()
            .min(candidate_acceptance.len().max(1));
        let (identities, accepted_candidate_count) = collect_identities_parallel(
            problem.initial_board().occupied_mask(),
            packing,
            candidate_acceptance,
            cancellation,
            worker_count,
        )?;
        let solution_set_hash =
            normalized_tiling_solution_set_hash_from_sorted_standard_board64_identities(
                &identities,
            );
        Ok(BuildUpSolutionSetSummary {
            contract: Self {
                identities,
                solution_set_hash,
            },
            accepted_candidate_count,
        })
    }

    pub(crate) fn unique_solution_count(&self) -> usize {
        self.identities.len()
    }

    pub(crate) fn solution_set_hash(&self) -> &str {
        &self.solution_set_hash
    }

    pub(crate) fn identities(&self) -> &[StandardBoard64TilingIdentity] {
        &self.identities
    }

    pub(crate) fn keys(&self) -> Vec<String> {
        self.identities
            .iter()
            .copied()
            .map(NormalizedTilingSolutionKey::from_standard_board64_identity)
            .map(|key| key.to_string())
            .collect()
    }

    pub(crate) const fn key_algorithm(&self) -> &'static str {
        NORMALIZED_TILING_SOLUTION_KEY_ALGORITHM
    }

    pub(crate) const fn hash_algorithm(&self) -> &'static str {
        NORMALIZED_TILING_SOLUTION_SET_HASH_ALGORITHM
    }
}

fn collect_identities_parallel(
    initial_board_mask: u64,
    packing: &PackingRunResult,
    candidate_acceptance: &BuildUpCandidateAcceptance,
    cancellation: &ExecutionCancellationToken,
    worker_count: usize,
) -> Result<(Vec<StandardBoard64TilingIdentity>, usize), BuildUpRunnerError> {
    if candidate_acceptance.is_empty() {
        return Ok((Vec::new(), 0));
    }
    let next_index = AtomicUsize::new(0);
    let partials = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            handles.push(scope.spawn(|| {
                let mut identities = Vec::with_capacity(SOLUTION_SET_CHUNK_SIZE);
                let mut accepted_count = 0_usize;
                loop {
                    if cancellation.is_cancelled() {
                        return Err(BuildUpRunnerError::ExecutionCancelled);
                    }
                    let start = next_index.fetch_add(SOLUTION_SET_CHUNK_SIZE, Ordering::Relaxed);
                    if start >= candidate_acceptance.len() {
                        break;
                    }
                    let end = start
                        .saturating_add(SOLUTION_SET_CHUNK_SIZE)
                        .min(candidate_acceptance.len());
                    for candidate_index in start..end {
                        let candidate = packing.candidate_view_at(candidate_index).ok_or(
                            BuildUpRunnerError::PackingCandidateUnavailable { candidate_index },
                        )?;
                        let candidate_id = candidate.candidate_id();
                        let Some(accepted) =
                            candidate_acceptance.candidate_accepted(candidate_index, candidate_id)
                        else {
                            let result_candidate_id = candidate_acceptance
                                .explicit_results()
                                .and_then(|results| results.get(candidate_index))
                                .map_or(0, |result| result.candidate_id);
                            return Err(BuildUpRunnerError::BuildUpCandidateIdentityMismatch {
                                candidate_index,
                                candidate_id,
                                result_candidate_id,
                            });
                        };
                        if !accepted {
                            continue;
                        }
                        let identity = candidate
                            .standard_board64_tiling_identity(initial_board_mask)
                            .map_err(map_identity_error)?;
                        if identities.len() == identities.capacity() {
                            identities
                                .try_reserve(SOLUTION_SET_CHUNK_SIZE)
                                .map_err(|_| BuildUpRunnerError::SolutionSetAllocationFailed)?;
                        }
                        identities.push(identity);
                        accepted_count = accepted_count.saturating_add(1);
                    }
                }
                Ok((identities, accepted_count))
            }));
        }
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .map_err(|_| BuildUpRunnerError::ParallelWorkerPanicked)?
            })
            .collect::<Result<Vec<_>, _>>()
    })?;

    let accepted_count = partials
        .iter()
        .map(|(_, accepted_count)| *accepted_count)
        .sum();
    let identity_count = partials
        .iter()
        .map(|(identities, _)| identities.len())
        .sum();
    let mut identity_sets = partials
        .into_iter()
        .map(|(identities, _)| identities)
        .collect::<Vec<_>>();
    let mut merged = Vec::new();
    merged
        .try_reserve_exact(identity_count)
        .map_err(|_| BuildUpRunnerError::SolutionSetAllocationFailed)?;
    for identities in &mut identity_sets {
        merged.append(identities);
    }
    merged.sort_unstable();
    merged.dedup();
    Ok((merged, accepted_count))
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
