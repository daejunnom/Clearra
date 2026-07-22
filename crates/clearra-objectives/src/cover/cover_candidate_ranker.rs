use clearra_coverage::matrix::coverage_matrix::TypedCoverageMatrix;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CoverCandidateRanker;

impl CoverCandidateRanker {
    pub fn rank_by_coverage_desc(matrix: &TypedCoverageMatrix) -> Vec<usize> {
        let mut row_indices = (0..matrix.rows().len()).collect::<Vec<_>>();
        row_indices.sort_by_key(|index| {
            let row = matrix.rows().get(*index).expect("row index is valid");
            (
                std::cmp::Reverse(row.coverage_bits().count_ones()),
                row.candidate_id(),
            )
        });
        row_indices
    }
}
