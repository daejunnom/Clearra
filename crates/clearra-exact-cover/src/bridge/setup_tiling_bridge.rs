use crate::{
    builder::cell_universe_builder::{CellUniverse, CellUniverseBuilder},
    model::{
        AreaConstraintColumn, ExactCoverCandidate, ExactCoverCandidateRow, ExactCoverColumn,
        ExactCoverProblem, ExactCoverProblemSchema, ExactCoverProblemSchemaError,
    },
    solver::{DlxSearchLimits, DlxSolver, DlxSolverResult},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SetupTilingBridge;

impl SetupTilingBridge {
    pub fn problem_from_shape_and_candidates(
        shape_mask: u64,
        candidate_masks: Vec<u64>,
    ) -> ExactCoverProblem {
        let Ok(universe) = CellUniverseBuilder::universe_from_mask(shape_mask) else {
            return ExactCoverProblem::new(0, Vec::new());
        };
        if let Ok(schema) =
            Self::problem_schema_from_shape_and_candidates(shape_mask, candidate_masks.clone())
        {
            return schema.to_problem();
        }
        let candidates = candidate_masks
            .into_iter()
            .enumerate()
            .map(|(id, mask)| {
                let columns = compact_columns_for_mask(mask, &universe);
                ExactCoverCandidate::new(id, columns)
            })
            .collect();
        ExactCoverProblem::new(universe.column_count(), candidates)
    }
}
impl SetupTilingBridge {
    pub fn problem_schema_from_shape_and_candidates(
        shape_mask: u64,
        candidate_masks: Vec<u64>,
    ) -> Result<ExactCoverProblemSchema, ExactCoverProblemSchemaError> {
        let Ok(universe) = CellUniverseBuilder::universe_from_mask(shape_mask) else {
            return ExactCoverProblemSchema::new(
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
        };
        let required_columns = universe
            .cells()
            .iter()
            .enumerate()
            .map(|(index, cell)| ExactCoverColumn::required(index, format!("cell:{cell}")))
            .collect::<Vec<_>>();
        let candidate_rows = candidate_masks
            .into_iter()
            .enumerate()
            .map(|(id, mask)| {
                ExactCoverCandidateRow::new(
                    id,
                    compact_columns_for_mask(mask, &universe),
                    Vec::new(),
                )
            })
            .collect::<Vec<_>>();

        ExactCoverProblemSchema::new(
            universe.cells().to_vec(),
            Vec::new(),
            Vec::new(),
            vec![AreaConstraintColumn::new(
                "setup-shape-area",
                universe.column_count(),
            )],
            required_columns,
            Vec::new(),
            candidate_rows,
        )
    }
}
impl SetupTilingBridge {
    pub fn enumerate(
        shape_mask: u64,
        candidate_masks: Vec<u64>,
        limits: DlxSearchLimits,
    ) -> DlxSolverResult {
        let problem = Self::problem_from_shape_and_candidates(shape_mask, candidate_masks);
        DlxSolver::solve_all_limited(&problem, limits)
    }
}

fn compact_columns_for_mask(mask: u64, universe: &CellUniverse) -> Vec<usize> {
    universe
        .cells()
        .iter()
        .copied()
        .filter(|absolute_index| (mask & (1_u64 << absolute_index)) != 0)
        .filter_map(|absolute_index| universe.compact_column_for_cell(absolute_index))
        .collect()
}

#[cfg(test)]
#[path = "setup_tiling_bridge_tests.rs"]
mod tests;
