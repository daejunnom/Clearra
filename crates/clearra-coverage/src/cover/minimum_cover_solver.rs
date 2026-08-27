use crate::{
    cover::{
        cover_selection::{
            CoverSelection, CoverSelectionLimit, CoverSelectionOptimality, CoverSelectionStrategy,
        },
        exact_minimum_cover::{
            exact_minimum_cover, exact_minimum_cover_with_memory_guard, ExactMinimumCoverError,
        },
        exact_minimum_cover_portfolios::{
            ExactMinimumCoverPortfolioEnumerator, ExactMinimumCoverPortfolioError,
        },
    },
    matrix::coverage_matrix::{CoverageMatrix, TypedCoverageMatrix},
    pattern::pattern_bitset::PatternBitSet,
};

pub const EXACT_MIN_COVER_ROW_LIMIT: usize = 20;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MinimumCoverSolver;

impl MinimumCoverSolver {
    /// Creates the unbounded exact, lazy all-optima authority over the original
    /// matrix row identities. Unlike [`Self::solve`], this never substitutes a
    /// greedy result after an arbitrary row-count threshold.
    pub fn exact_portfolios(
        matrix: &CoverageMatrix,
        required: &PatternBitSet,
    ) -> Result<ExactMinimumCoverPortfolioEnumerator, ExactMinimumCoverPortfolioError> {
        ExactMinimumCoverPortfolioEnumerator::new(
            required,
            &matrix
                .rows()
                .iter()
                .map(|row| row.patterns().clone())
                .collect::<Vec<_>>(),
        )
    }

    pub fn exact_typed_portfolios(
        matrix: &TypedCoverageMatrix,
        required: &PatternBitSet,
    ) -> Result<ExactMinimumCoverPortfolioEnumerator, ExactMinimumCoverPortfolioError> {
        ExactMinimumCoverPortfolioEnumerator::new(
            required,
            &matrix
                .rows()
                .iter()
                .map(|row| row.coverage_bits().clone())
                .collect::<Vec<_>>(),
        )
    }

    pub fn solve(matrix: &TypedCoverageMatrix, required: &PatternBitSet) -> CoverSelection {
        if required.is_empty() {
            return CoverSelection::exact_minimum(
                Vec::new(),
                PatternBitSet::new(matrix.pattern_count()),
            );
        }

        if matrix.rows().len() <= EXACT_MIN_COVER_ROW_LIMIT {
            return bounded_exact_minimum_cover(matrix, required);
        }

        greedy_cover(matrix, required)
    }

    /// Runs the shared unbounded exact solver over a matrix-owned row set.
    ///
    /// This is the authority used by coverage-class portfolio selection. It deliberately does
    /// not fall back to the greedy policy used by [`Self::solve`] for interactive bounded work.
    pub fn solve_exact(
        matrix: &CoverageMatrix,
        required: &PatternBitSet,
    ) -> Result<CoverSelection, ExactMinimumCoverError> {
        let result = exact_minimum_cover(
            required,
            &matrix
                .rows()
                .iter()
                .map(|row| row.patterns().clone())
                .collect::<Vec<_>>(),
        )?;
        Ok(exact_result_to_selection(result))
    }

    /// Runs the shared exact authority with the matrix-row adapter and every
    /// exact-search allocation governed by one numeric memory limit.
    pub fn solve_exact_with_memory_limit(
        matrix: &CoverageMatrix,
        required: &PatternBitSet,
        already_retained_bytes: u128,
        max_memory_bytes: u128,
    ) -> Result<CoverSelection, ExactMinimumCoverError> {
        Self::solve_exact_with_memory_guard(matrix, required, &mut |owned_bytes| {
            ensure_memory_limit(already_retained_bytes, owned_bytes, max_memory_bytes)
        })
    }

    /// Reports the matrix adapter plus exact solver's requested and actual
    /// owned capacities to one caller-supplied authority.
    pub fn solve_exact_with_memory_guard(
        matrix: &CoverageMatrix,
        required: &PatternBitSet,
        memory_guard: &mut impl FnMut(u128) -> Result<(), ExactMinimumCoverError>,
    ) -> Result<CoverSelection, ExactMinimumCoverError> {
        memory_guard(0)?;
        let row_count = matrix.rows().len();
        let requested_row_bytes = (row_count as u128)
            .checked_mul(core::mem::size_of::<PatternBitSet>() as u128)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        memory_guard(requested_row_bytes)?;
        let mut rows = Vec::new();
        rows.try_reserve_exact(row_count).map_err(|_| {
            ExactMinimumCoverError::AllocationFailed {
                component: "minimum_cover_solver_matrix_rows",
            }
        })?;
        let actual_row_bytes = (rows.capacity() as u128)
            .checked_mul(core::mem::size_of::<PatternBitSet>() as u128)
            .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
        memory_guard(actual_row_bytes)?;
        rows.extend(matrix.rows().iter().map(|row| row.patterns().clone()));
        let result = exact_minimum_cover_with_memory_guard(required, &rows, &mut |exact_bytes| {
            memory_guard(
                actual_row_bytes
                    .checked_add(exact_bytes)
                    .ok_or(ExactMinimumCoverError::ProjectionOverflow)?,
            )
        })?;
        drop(rows);
        Ok(exact_result_to_selection(result))
    }
}

fn exact_result_to_selection(
    result: crate::cover::exact_minimum_cover::ExactMinimumCoverResult,
) -> CoverSelection {
    let (row_indices, covered_patterns, complete) = result.into_parts();
    CoverSelection::new(
        row_indices,
        covered_patterns,
        complete,
        CoverSelectionStrategy::ExactSearch,
        if complete {
            CoverSelectionOptimality::ProvenMinimum
        } else {
            CoverSelectionOptimality::NoCompleteCover
        },
        CoverSelectionLimit::None,
    )
}

fn ensure_memory_limit(
    already_retained_bytes: u128,
    future_bytes: u128,
    max_memory_bytes: u128,
) -> Result<(), ExactMinimumCoverError> {
    let required_memory_bytes = already_retained_bytes
        .checked_add(future_bytes)
        .ok_or(ExactMinimumCoverError::ProjectionOverflow)?;
    if required_memory_bytes > max_memory_bytes {
        return Err(ExactMinimumCoverError::MemoryCapacityExceeded {
            required_memory_bytes,
            max_memory_bytes,
        });
    }
    Ok(())
}

fn bounded_exact_minimum_cover(
    matrix: &TypedCoverageMatrix,
    required: &PatternBitSet,
) -> CoverSelection {
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
