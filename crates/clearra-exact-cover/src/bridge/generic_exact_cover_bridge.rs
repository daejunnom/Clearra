use crate::{
    builder::{
        cell_universe_builder::CellUniverse, piece_area_constraint::PieceAreaConstraint,
        PieceAreaConstraintError,
    },
    model::{
        AreaConstraintColumn, ExactCoverCandidateRow, ExactCoverColumn, ExactCoverProblem,
        ExactCoverProblemSchema, ExactCoverProblemSchemaError, GenericExactCoverCandidate,
        PieceUsageConstraint,
    },
    solver::{DlxSearchLimits, DlxSolveReport, DlxSolver, DlxSolverError},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GenericExactCoverBridgeError {
    PieceArea(PieceAreaConstraintError),
    AreaInfeasibleShape {
        target_area: usize,
        available_piece_areas: Vec<usize>,
    },
    Dlx(DlxSolverError),
    Schema(ExactCoverProblemSchemaError),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GenericExactCoverBridge;

impl GenericExactCoverBridge {
    pub fn problem_from_candidates(
        universe: &CellUniverse,
        candidates: &[GenericExactCoverCandidate],
    ) -> ExactCoverProblem {
        ExactCoverProblem::new(
            universe.column_count(),
            candidates
                .iter()
                .map(GenericExactCoverCandidate::to_exact_cover_candidate)
                .collect(),
        )
    }
}
impl GenericExactCoverBridge {
    pub fn problem_schema_from_candidates(
        universe: &CellUniverse,
        candidates: &[GenericExactCoverCandidate],
        piece_usage_constraints: Vec<PieceUsageConstraint>,
        area_constraints: Vec<AreaConstraintColumn>,
    ) -> Result<ExactCoverProblemSchema, GenericExactCoverBridgeError> {
        let required_columns = universe
            .cells()
            .iter()
            .enumerate()
            .map(|(index, cell)| ExactCoverColumn::required(index, format!("cell:{cell}")))
            .collect::<Vec<_>>();
        let candidate_rows = candidates
            .iter()
            .map(|candidate| {
                ExactCoverCandidateRow::new(
                    candidate.candidate_id(),
                    candidate.columns().to_vec(),
                    Vec::new(),
                )
            })
            .collect::<Vec<_>>();

        ExactCoverProblemSchema::new(
            universe.cells().to_vec(),
            piece_usage_constraints,
            Vec::new(),
            area_constraints,
            required_columns,
            Vec::new(),
            candidate_rows,
        )
        .map_err(GenericExactCoverBridgeError::Schema)
    }
}
impl GenericExactCoverBridge {
    pub fn enumerate_tilings(
        universe: &CellUniverse,
        candidates: &[GenericExactCoverCandidate],
        available_piece_areas: impl IntoIterator<Item = usize>,
        limits: DlxSearchLimits,
    ) -> Result<DlxSolveReport, GenericExactCoverBridgeError> {
        let available_piece_areas = available_piece_areas.into_iter().collect::<Vec<_>>();
        let area_constraint =
            PieceAreaConstraint::new(universe.column_count(), available_piece_areas.clone())
                .map_err(GenericExactCoverBridgeError::PieceArea)?;
        if !area_constraint.can_fill_target() {
            return Err(GenericExactCoverBridgeError::AreaInfeasibleShape {
                target_area: area_constraint.target_area(),
                available_piece_areas,
            });
        }

        let problem = Self::problem_schema_from_candidates(
            universe,
            candidates,
            Vec::new(),
            vec![AreaConstraintColumn::new(
                "cell-universe-area",
                universe.column_count(),
            )],
        )?
        .to_problem();
        DlxSolver::solve_all_limited(&problem, limits).map_err(GenericExactCoverBridgeError::Dlx)
    }
}

#[cfg(test)]
#[path = "generic_exact_cover_bridge_tests.rs"]
mod tests;
