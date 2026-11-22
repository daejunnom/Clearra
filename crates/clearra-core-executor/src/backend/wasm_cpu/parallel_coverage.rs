use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use clearra_coverage::pattern::pattern_bitset::PatternBitSet;

use super::WasmExactSearchError;

pub(super) struct SharedCoverage {
    pattern_count: usize,
    words: Vec<AtomicU64>,
    complete: AtomicBool,
}

impl SharedCoverage {
    pub fn new(pattern_count: usize) -> Self {
        Self {
            pattern_count,
            words: (0..pattern_count.div_ceil(u64::BITS as usize))
                .map(|_| AtomicU64::new(0))
                .collect(),
            complete: AtomicBool::new(pattern_count == 0),
        }
    }

    pub fn union(&self, coverage: &PatternBitSet) -> Result<(), WasmExactSearchError> {
        if coverage.pattern_count() != self.pattern_count {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_parallel_coverage_universe_mismatch",
            ));
        }
        self.union_valid_words(coverage.words());
        Ok(())
    }

    /// Borrowed final worker coverage, on the same original universe. Validate
    /// the entire shape before any monotone atomic publication.
    pub(super) fn union_original_words(
        &self,
        pattern_count: usize,
        coverage: Option<&[u64]>,
    ) -> Result<(), WasmExactSearchError> {
        if pattern_count != self.pattern_count {
            return Err(WasmExactSearchError::InvalidProblem(
                "wasm_parallel_coverage_universe_mismatch",
            ));
        }
        if let Some(words) = coverage {
            if words.len() != self.words.len() {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_parallel_coverage_word_shape_mismatch",
                ));
            }
            self.union_valid_words(words);
        }
        Ok(())
    }

    fn union_valid_words(&self, coverage: &[u64]) {
        let (Some((&last_word, coverage)), Some((last_destination, destinations))) =
            (coverage.split_last(), self.words.split_last())
        else {
            return;
        };
        for (shared, word) in destinations.iter().zip(coverage.iter().copied()) {
            shared.fetch_or(word, Ordering::AcqRel);
        }
        // Match PatternBitSet::from_words when the borrowed source spans only
        // part of its last storage word.
        last_destination.fetch_or(last_word & self.tail_mask(), Ordering::AcqRel);
        if self.is_complete() {
            self.complete.store(true, Ordering::Release);
        }
    }

    pub fn is_superset(&self, required: &PatternBitSet) -> bool {
        required.pattern_count() == self.pattern_count
            && self
                .words
                .iter()
                .zip(required.words().iter().copied())
                .all(|(shared, required)| shared.load(Ordering::Acquire) & required == required)
    }

    pub fn to_bitset(&self) -> Result<PatternBitSet, WasmExactSearchError> {
        PatternBitSet::from_words(
            self.pattern_count,
            self.words
                .iter()
                .map(|word| word.load(Ordering::Acquire))
                .collect(),
        )
        .map_err(|_| WasmExactSearchError::InvalidProblem("wasm_parallel_coverage_invalid"))
    }

    fn is_complete(&self) -> bool {
        if self.complete.load(Ordering::Acquire) {
            return true;
        }
        self.words.iter().enumerate().all(|(index, word)| {
            let expected = if index + 1 == self.words.len() {
                self.tail_mask()
            } else {
                u64::MAX
            };
            word.load(Ordering::Acquire) == expected
        })
    }

    fn tail_mask(&self) -> u64 {
        match self.pattern_count % u64::BITS as usize {
            0 => u64::MAX,
            remainder => (1_u64 << remainder) - 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_borrowed_union_masks_tail_and_rejects_malformed_shapes() {
        let direct = SharedCoverage::new(129);
        let materialized = SharedCoverage::new(129);
        direct.union_original_words(129, None).expect("empty union");
        assert!(direct.to_bitset().expect("valid universe").is_empty());
        for words in [[3, 0, u64::MAX], [!3, u64::MAX, 0], [3, 0, u64::MAX]] {
            direct
                .union_original_words(129, Some(&words))
                .expect("borrowed");
            materialized
                .union(&PatternBitSet::from_words(129, words.to_vec()).expect("valid shape"))
                .expect("existing materialized union");
            assert_eq!(
                direct.to_bitset().expect("borrowed result"),
                materialized.to_bitset().expect("materialized result")
            );
        }
        assert!(direct.is_complete());
        assert_eq!(direct.words[2].load(Ordering::Acquire), 1);

        let unchanged = SharedCoverage::new(129);
        assert!(matches!(
            unchanged.union_original_words(130, Some(&[1, 2, 3])),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_parallel_coverage_universe_mismatch"
            ))
        ));
        assert!(matches!(
            unchanged.union_original_words(129, Some(&[1, 2])),
            Err(WasmExactSearchError::InvalidProblem(
                "wasm_parallel_coverage_word_shape_mismatch"
            ))
        ));
        assert!(unchanged.to_bitset().expect("valid result").is_empty());
    }
}
