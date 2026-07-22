use std::collections::BTreeMap;

use clearra_exact_cover::model::ExactCoverSolution;
use clearra_problem::SearchProblem;

use crate::packing_problem::{CPackingCandidate, CPackingOperation, C_PACKING_MAX_OPERATIONS};

use super::{CBuildUpProblem, CBuildUpProblemBuilder, FfiProblemError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DlxBuildUpOperationCandidate {
    candidate_id: usize,
    operation: CPackingOperation,
}

impl DlxBuildUpOperationCandidate {
    pub fn new(candidate_id: usize, operation: CPackingOperation) -> Self {
        Self {
            candidate_id,
            operation,
        }
    }
}
impl DlxBuildUpOperationCandidate {
    pub fn candidate_id(self) -> usize {
        self.candidate_id
    }
}
impl DlxBuildUpOperationCandidate {
    pub fn operation(self) -> CPackingOperation {
        self.operation
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DlxBuildUpBridgeError {
    MissingCandidate { candidate_id: usize },
    TooManyOperations { selected_count: usize },
    Ffi(FfiProblemError),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DlxBuildUpBridge;

impl DlxBuildUpBridge {
    pub fn dlx_solution_is_not_build_variant() -> bool {
        true
    }
}
impl DlxBuildUpBridge {
    pub fn packing_candidate_from_solution(
        solution: &ExactCoverSolution,
        operation_candidates: &[DlxBuildUpOperationCandidate],
    ) -> Result<CPackingCandidate, DlxBuildUpBridgeError> {
        let selected_count = solution.candidate_ids().len();
        if selected_count > C_PACKING_MAX_OPERATIONS {
            return Err(DlxBuildUpBridgeError::TooManyOperations { selected_count });
        }

        let by_id = operation_candidates
            .iter()
            .copied()
            .map(|candidate| (candidate.candidate_id(), candidate.operation()))
            .collect::<BTreeMap<_, _>>();

        let mut packing = CPackingCandidate {
            candidate_id: solution_candidate_key(solution),
            canonical_operation_set_id: solution_candidate_key(solution),
            operation_count: selected_count as u16,
            ..Default::default()
        };

        for (index, candidate_id) in solution.candidate_ids().iter().copied().enumerate() {
            let operation = by_id
                .get(&candidate_id)
                .copied()
                .ok_or(DlxBuildUpBridgeError::MissingCandidate { candidate_id })?;
            packing.operations[index] = operation;
            packing.shape_mask |= operation.mask;
            packing.final_board |= operation.mask;
        }
        packing.shape_key = packing.shape_mask;
        packing.tiling_key = solution_candidate_key(solution);
        packing.operation_set_key = solution_candidate_key(solution).rotate_left(17);

        Ok(packing)
    }
}
impl DlxBuildUpBridge {
    pub fn buildup_problem_from_solution(
        problem: &SearchProblem,
        solution: &ExactCoverSolution,
        operation_candidates: &[DlxBuildUpOperationCandidate],
        coverage_pattern_id: u32,
    ) -> Result<CBuildUpProblem, DlxBuildUpBridgeError> {
        let packing = Self::packing_candidate_from_solution(solution, operation_candidates)?;
        CBuildUpProblemBuilder::from_packing_candidate(problem, &packing, 0, coverage_pattern_id)
            .map_err(DlxBuildUpBridgeError::Ffi)
    }
}

fn solution_candidate_key(solution: &ExactCoverSolution) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for candidate_id in solution.candidate_ids() {
        hash ^= *candidate_id as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
#[path = "dlx_buildup_bridge_tests.rs"]
mod tests;
