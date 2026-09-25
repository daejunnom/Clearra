//! Exact minimum cover conditioned on mandatory original-row identities.
//!
//! Each pinned row gets a private requirement bit. The ordinary exact solver
//! therefore proves the minimum cardinality *subject to* every pin being
//! present, and its existing lexicographic portfolio authority continues to
//! enumerate original row IDs. Filtering unconstrained optima after the fact
//! would miss valid, larger conditional optima.

use super::exact_minimum_cover_portfolios::{
    ExactMinimumCoverPortfolioEnumerator, ExactMinimumCoverPortfolioError,
};
use crate::pattern::pattern_bitset::PatternBitSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PinnedMinimumCoverError {
    PinnedRowOutOfRange { row_index: usize, row_count: usize },
    DuplicatePinnedRow { row_index: usize },
    RowPatternCountMismatch { row_index: usize },
    PatternCountOverflow,
    AllocationFailed,
    InvalidAugmentedPatternSet,
    Portfolio(ExactMinimumCoverPortfolioError),
    CanonicalPortfolioMissing,
    PinnedRowMissingFromProof { row_index: usize },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PinnedMinimumCoverPortfolio {
    pinned_row_indices: Vec<usize>,
    additional_row_indices: Vec<usize>,
    all_row_indices: Vec<usize>,
}

impl PinnedMinimumCoverPortfolio {
    pub fn pinned_row_indices(&self) -> &[usize] {
        &self.pinned_row_indices
    }

    pub fn additional_row_indices(&self) -> &[usize] {
        &self.additional_row_indices
    }

    pub fn all_row_indices(&self) -> &[usize] {
        &self.all_row_indices
    }
}

/// A source-bound conditional set-cover problem. `required` is the same
/// original coverage universe as an ordinary minimum-cover run; private bits
/// are never exposed as queue coverage or probability evidence.
#[derive(Clone, Debug)]
pub struct PinnedMinimumCoverInput {
    required: PatternBitSet,
    rows: Vec<PatternBitSet>,
    pinned_row_indices: Vec<usize>,
    augmented_required: PatternBitSet,
    augmented_rows: Vec<PatternBitSet>,
}

impl PinnedMinimumCoverInput {
    pub fn new(
        required: PatternBitSet,
        rows: Vec<PatternBitSet>,
        mut pinned_row_indices: Vec<usize>,
    ) -> Result<Self, PinnedMinimumCoverError> {
        for (row_index, row) in rows.iter().enumerate() {
            if row.pattern_count() != required.pattern_count() {
                return Err(PinnedMinimumCoverError::RowPatternCountMismatch { row_index });
            }
        }
        pinned_row_indices.sort_unstable();
        for (position, &row_index) in pinned_row_indices.iter().enumerate() {
            if row_index >= rows.len() {
                return Err(PinnedMinimumCoverError::PinnedRowOutOfRange {
                    row_index,
                    row_count: rows.len(),
                });
            }
            if position > 0 && pinned_row_indices[position - 1] == row_index {
                return Err(PinnedMinimumCoverError::DuplicatePinnedRow { row_index });
            }
        }

        let augmented_count = required
            .pattern_count()
            .checked_add(pinned_row_indices.len())
            .ok_or(PinnedMinimumCoverError::PatternCountOverflow)?;
        let augmented_word_count = augmented_count.div_ceil(u64::BITS as usize);
        let mut augmented_required_words = required.to_owned_words();
        augmented_required_words
            .try_reserve(augmented_word_count.saturating_sub(augmented_required_words.len()))
            .map_err(|_| PinnedMinimumCoverError::AllocationFailed)?;
        augmented_required_words.resize(augmented_word_count, 0);
        for pin_ordinal in 0..pinned_row_indices.len() {
            let bit = required.pattern_count() + pin_ordinal;
            augmented_required_words[bit / u64::BITS as usize] |=
                1_u64 << (bit % u64::BITS as usize);
        }
        let augmented_required =
            PatternBitSet::from_words(augmented_count, augmented_required_words)
                .map_err(|_| PinnedMinimumCoverError::InvalidAugmentedPatternSet)?;

        let mut augmented_rows = Vec::new();
        augmented_rows
            .try_reserve_exact(rows.len())
            .map_err(|_| PinnedMinimumCoverError::AllocationFailed)?;
        for (row_index, row) in rows.iter().enumerate() {
            let mut words = row.to_owned_words();
            words
                .try_reserve(augmented_word_count.saturating_sub(words.len()))
                .map_err(|_| PinnedMinimumCoverError::AllocationFailed)?;
            words.resize(augmented_word_count, 0);
            if let Ok(pin_ordinal) = pinned_row_indices.binary_search(&row_index) {
                let bit = required.pattern_count() + pin_ordinal;
                words[bit / u64::BITS as usize] |= 1_u64 << (bit % u64::BITS as usize);
            }
            augmented_rows.push(
                PatternBitSet::from_words(augmented_count, words)
                    .map_err(|_| PinnedMinimumCoverError::InvalidAugmentedPatternSet)?,
            );
        }
        Ok(Self {
            required,
            rows,
            pinned_row_indices,
            augmented_required,
            augmented_rows,
        })
    }

    pub fn required_patterns(&self) -> &PatternBitSet {
        &self.required
    }

    pub fn coverage_rows(&self) -> &[PatternBitSet] {
        &self.rows
    }

    pub fn pinned_row_indices(&self) -> &[usize] {
        &self.pinned_row_indices
    }

    /// Transfers the private exact-search matrix to a product-side pager.
    /// Its row indices still refer to the caller's original candidate order.
    pub fn into_augmented_parts(self) -> (PatternBitSet, Vec<PatternBitSet>) {
        (self.augmented_required, self.augmented_rows)
    }

    /// The returned cursor retains the private requirements and original row
    /// numbering. Its pages are exact all-optima for the conditional problem.
    pub fn exact_portfolios(
        &self,
    ) -> Result<ExactMinimumCoverPortfolioEnumerator, PinnedMinimumCoverError> {
        ExactMinimumCoverPortfolioEnumerator::new(&self.augmented_required, &self.augmented_rows)
            .map_err(PinnedMinimumCoverError::Portfolio)
    }

    pub fn canonical_portfolio(
        &self,
    ) -> Result<PinnedMinimumCoverPortfolio, PinnedMinimumCoverError> {
        let portfolio = self
            .exact_portfolios()?
            .into_canonical_portfolio()
            .map_err(PinnedMinimumCoverError::Portfolio)?
            .ok_or(PinnedMinimumCoverError::CanonicalPortfolioMissing)?;
        let all_row_indices = portfolio.into_row_indices();
        for &row_index in &self.pinned_row_indices {
            if all_row_indices.binary_search(&row_index).is_err() {
                return Err(PinnedMinimumCoverError::PinnedRowMissingFromProof { row_index });
            }
        }
        let additional_row_indices = all_row_indices
            .iter()
            .copied()
            .filter(|row_index| self.pinned_row_indices.binary_search(row_index).is_err())
            .collect();
        Ok(PinnedMinimumCoverPortfolio {
            pinned_row_indices: self.pinned_row_indices.clone(),
            additional_row_indices,
            all_row_indices,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pattern::pattern_id::PatternId;

    fn bits(indices: impl IntoIterator<Item = usize>) -> PatternBitSet {
        PatternBitSet::from_patterns(3, indices.into_iter().map(PatternId::new)).unwrap()
    }

    #[test]
    fn pinned_redundant_row_is_retained_and_additional_cover_is_reoptimized() {
        let input = PinnedMinimumCoverInput::new(
            bits(0..3),
            vec![bits([0, 1]), bits([1, 2]), bits([0]), bits([2])],
            vec![2, 0],
        )
        .unwrap();
        let canonical = input.canonical_portfolio().unwrap();
        assert_eq!(canonical.pinned_row_indices(), [0, 2]);
        assert_eq!(canonical.additional_row_indices(), [1]);
        assert_eq!(canonical.all_row_indices(), [0, 1, 2]);
    }

    #[test]
    fn conditional_ties_keep_distinct_original_row_identities() {
        let input = PinnedMinimumCoverInput::new(
            bits(0..3),
            vec![bits([0]), bits([1]), bits([1]), bits([2])],
            vec![0, 3],
        )
        .unwrap();
        let mut portfolios = input.exact_portfolios().unwrap();
        let page = portfolios.next_page(10, u64::MAX).unwrap();
        assert_eq!(
            page.portfolios()
                .iter()
                .map(|portfolio| portfolio.row_indices().to_vec())
                .collect::<Vec<_>>(),
            [vec![0, 1, 3], vec![0, 2, 3]]
        );
        assert_eq!(page.total_alternative_count_decimal(), Some("2"));
    }

    #[test]
    fn zero_coverage_pin_is_retained_and_invalid_pin_is_rejected() {
        let required = bits([0]);
        let rows = vec![bits([0]), bits([1])];
        let pinned = PinnedMinimumCoverInput::new(required.clone(), rows.clone(), vec![1]).unwrap();
        assert_eq!(
            pinned.canonical_portfolio().unwrap().all_row_indices(),
            [0, 1]
        );
        assert_eq!(
            PinnedMinimumCoverInput::new(required, rows, vec![0, 0]).unwrap_err(),
            PinnedMinimumCoverError::DuplicatePinnedRow { row_index: 0 }
        );
    }
}
