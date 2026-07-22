use crate::{
    cover::cover_selection::CoverSelection, matrix::coverage_matrix::TypedCoverageMatrix,
    pattern::pattern_bitset::PatternBitSet,
};

pub const EXACT_MIN_COVER_ROW_LIMIT: usize = 20;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MinimumCoverSolver;

impl MinimumCoverSolver {
    pub fn solve(matrix: &TypedCoverageMatrix, required: &PatternBitSet) -> CoverSelection {
        if required.is_empty() {
            return CoverSelection::exact_minimum(
                Vec::new(),
                PatternBitSet::new(matrix.pattern_count()),
            );
        }

        if matrix.rows().len() <= EXACT_MIN_COVER_ROW_LIMIT {
            return exact_minimum_cover(matrix, required);
        }

        greedy_cover(matrix, required)
    }
}

fn exact_minimum_cover(matrix: &TypedCoverageMatrix, required: &PatternBitSet) -> CoverSelection {
    for size in 1..=matrix.rows().len() {
        let mut current = Vec::with_capacity(size);
        if let Some(selection) = choose_rows(matrix, required, size, 0, &mut current) {
            return selection;
        }
    }

    CoverSelection::exact_incomplete(PatternBitSet::new(matrix.pattern_count()))
}

fn choose_rows(
    matrix: &TypedCoverageMatrix,
    required: &PatternBitSet,
    remaining: usize,
    start: usize,
    current: &mut Vec<usize>,
) -> Option<CoverSelection> {
    if remaining == 0 {
        let covered = matrix
            .union_rows(current)
            .expect("minimum cover solver only selects matrix-owned row indices");
        return covered
            .is_superset(required)
            .expect("minimum cover required patterns must share matrix pattern universe")
            .then(|| CoverSelection::exact_minimum(current.clone(), covered));
    }

    let max_start = matrix.rows().len().saturating_sub(remaining);
    for index in start..=max_start {
        current.push(index);
        if let Some(selection) = choose_rows(matrix, required, remaining - 1, index + 1, current) {
            return Some(selection);
        }
        current.pop();
    }

    None
}

fn greedy_cover(matrix: &TypedCoverageMatrix, required: &PatternBitSet) -> CoverSelection {
    let mut selected = Vec::new();
    let mut covered = PatternBitSet::new(matrix.pattern_count());
    let mut remaining = (0..matrix.rows().len()).collect::<Vec<_>>();

    while !covered
        .is_superset(required)
        .expect("minimum cover required patterns must share matrix pattern universe")
        && !remaining.is_empty()
    {
        let best_position = remaining
            .iter()
            .enumerate()
            .max_by_key(|(_, row_index)| {
                let candidate = covered
                    .union(
                        matrix
                            .rows()
                            .get(**row_index)
                            .expect("row exists")
                            .coverage_bits(),
                    )
                    .expect("coverage matrix row pattern_count invariant");
                candidate.count_ones()
            })
            .map(|(position, _)| position);

        let Some(position) = best_position else {
            break;
        };

        let row_index = remaining.remove(position);
        covered
            .union_with(
                matrix
                    .rows()
                    .get(row_index)
                    .expect("row exists")
                    .coverage_bits(),
            )
            .expect("coverage matrix row pattern_count invariant");
        selected.push(row_index);
    }

    let complete = covered
        .is_superset(required)
        .expect("minimum cover required patterns must share matrix pattern universe");
    CoverSelection::greedy_fallback(
        selected,
        covered,
        complete,
        matrix.rows().len(),
        EXACT_MIN_COVER_ROW_LIMIT,
    )
}

#[cfg(test)]
#[path = "minimum_cover_solver_tests.rs"]
mod tests;
