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
        for (shared, word) in self.words.iter().zip(coverage.words().iter().copied()) {
            shared.fetch_or(word, Ordering::AcqRel);
        }
        if self.is_complete() {
            self.complete.store(true, Ordering::Release);
        }
        Ok(())
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
            let expected =
                if index + 1 == self.words.len() && self.pattern_count % u64::BITS as usize != 0 {
                    (1_u64 << (self.pattern_count % u64::BITS as usize)) - 1
                } else {
                    u64::MAX
                };
            word.load(Ordering::Acquire) == expected
        })
    }
}
