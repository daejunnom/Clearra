use clearra_coverage::{
    cover::{
        cover_selection::CoverSelection, minimum_cover_solver::MinimumCoverSolver,
        ExactMinimumCoverPortfolioEnumerator, ExactMinimumCoverPortfolioError,
    },
    matrix::coverage_matrix::TypedCoverageMatrix,
    pattern::pattern_bitset::PatternBitSet,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MinimumCoverObjective;

impl MinimumCoverObjective {
    pub fn select(matrix: &TypedCoverageMatrix, required: &PatternBitSet) -> CoverSelection {
        MinimumCoverSolver::solve(matrix, required)
    }

    pub fn portfolios(
        matrix: &TypedCoverageMatrix,
        required: &PatternBitSet,
    ) -> Result<ExactMinimumCoverPortfolioEnumerator, ExactMinimumCoverPortfolioError> {
        MinimumCoverSolver::exact_typed_portfolios(matrix, required)
    }
}

#[cfg(test)]
mod tests {
    use clearra_coverage::{
        cover::{CoverSelectionOptimality, CoverSelectionStrategy},
        matrix::coverage_matrix::TypedCoverageMatrix,
        pattern::{pattern_bitset::PatternBitSet, pattern_id::PatternId},
        row::{coverage_row::CoverageRow, coverage_row_kind::CoverageRowKind},
        universe::{
            pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
        },
    };

    use super::*;

    #[test]
    fn minimum_cover_objective_delegates_to_coverage_solver() {
        let required = PatternBitSet::from_patterns(2, [PatternId::new(0), PatternId::new(1)])
            .expect("required");
        let matrix = TypedCoverageMatrix::from_rows(
            CoverageRowKind::Pc,
            PatternUniverseId::new(1),
            PatternWeightModelId::new(7),
            2,
            vec![CoverageRow::new_with_piece_source(
                0,
                CoverageRowKind::Pc,
                11,
                PatternUniverseId::new(1),
                PatternWeightModelId::new(7),
                PatternBitSet::from_patterns(2, [PatternId::new(0), PatternId::new(1)])
                    .expect("row"),
            )],
        )
        .expect("matrix");

        let selection = MinimumCoverObjective::select(&matrix, &required);

        assert!(selection.is_complete());
        assert!(selection.is_proven_minimum());
        assert_eq!(selection.strategy(), CoverSelectionStrategy::ExactSearch);
        assert_eq!(
            selection.optimality(),
            CoverSelectionOptimality::ProvenMinimum
        );
        assert_eq!(selection.row_indices(), &[0]);
    }
}
