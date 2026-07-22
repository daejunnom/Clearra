use crate::{
    model::{ExactCoverCandidate, ExactCoverProblem},
    solver::{DlxSearchLimits, DlxSolveReport, DlxSolver, DlxSolverError},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomPiecePlacementColumns {
    placement_id: usize,
    columns: Vec<usize>,
}

impl CustomPiecePlacementColumns {
    pub fn new(placement_id: usize, columns: Vec<usize>) -> Self {
        Self {
            placement_id,
            columns,
        }
    }
}
impl CustomPiecePlacementColumns {
    pub fn placement_id(&self) -> usize {
        self.placement_id
    }
}
impl CustomPiecePlacementColumns {
    pub fn columns(&self) -> &[usize] {
        &self.columns
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CustomPieceBridge;

impl CustomPieceBridge {
    pub fn problem_from_placements(
        target_cell_count: usize,
        placements: Vec<CustomPiecePlacementColumns>,
    ) -> Result<ExactCoverProblem, CustomPieceBridgeError> {
        if target_cell_count == 0 {
            return Err(CustomPieceBridgeError::ZeroTargetCells);
        }

        let candidates = placements
            .into_iter()
            .map(|placement| ExactCoverCandidate::new(placement.placement_id, placement.columns))
            .collect();
        Ok(ExactCoverProblem::new(target_cell_count, candidates))
    }
}
impl CustomPieceBridge {
    pub fn enumerate_tilings(
        target_cell_count: usize,
        placements: Vec<CustomPiecePlacementColumns>,
        limits: DlxSearchLimits,
    ) -> Result<DlxSolveReport, CustomPieceBridgeError> {
        let problem = Self::problem_from_placements(target_cell_count, placements)?;
        DlxSolver::solve_all_limited(&problem, limits).map_err(CustomPieceBridgeError::Dlx)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CustomPieceBridgeError {
    ZeroTargetCells,
    Dlx(DlxSolverError),
}

#[cfg(test)]
#[path = "custom_piece_bridge_tests.rs"]
mod tests;
