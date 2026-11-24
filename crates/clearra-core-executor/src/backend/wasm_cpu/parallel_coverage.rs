use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
#[cfg(feature = "placement-dependency-pc")]
use std::sync::Arc;

use clearra_coverage::pattern::pattern_bitset::PatternBitSet;

#[cfg(feature = "placement-dependency-pc")]
use super::standard_bag_coverage::SharedStandardBagRequest;
use super::WasmExactSearchError;

pub(super) struct SharedCoverage {
    pattern_count: usize,
    words: Vec<AtomicU64>,
    complete: AtomicBool,
    #[cfg(feature = "placement-dependency-pc")]
    standard_bag_request: Option<Arc<SharedStandardBagRequest>>,
}

/// The exact, monotonically published bitmap owner is the only constructor.
/// Borrowing the proof allocates nothing and does not increment an Arc.
#[cfg(feature = "placement-dependency-pc")]
pub(super) struct SharedStandardBagCoverageComplete<'a> {
    request: &'a Arc<SharedStandardBagRequest>,
    pattern_count: usize,
}

#[cfg(feature = "placement-dependency-pc")]
impl SharedStandardBagCoverageComplete<'_> {
    pub fn request(&self) -> &Arc<SharedStandardBagRequest> {
        self.request
    }

    pub const fn pattern_count(&self) -> usize {
        self.pattern_count
    }
}

impl SharedCoverage {
    pub fn new(pattern_count: usize) -> Self {
        Self {
            pattern_count,
            words: (0..pattern_count.div_ceil(u64::BITS as usize))
                .map(|_| AtomicU64::new(0))
                .collect(),
            complete: AtomicBool::new(pattern_count == 0),
            #[cfg(feature = "placement-dependency-pc")]
            standard_bag_request: None,
        }
    }

    #[cfg(feature = "placement-dependency-pc")]
    pub(super) fn for_standard_bag_request(
        pattern_count: usize,
        request: Arc<SharedStandardBagRequest>,
    ) -> Self {
        let mut coverage = Self::new(pattern_count);
        // Bind before any publication. An already full unbound bitmap cannot
        // be attached to another request and turned into a retirement proof.
        coverage.standard_bag_request = Some(request);
        coverage
    }

    #[cfg(feature = "placement-dependency-pc")]
    pub(super) fn standard_bag_complete_proof(
        &self,
    ) -> Option<SharedStandardBagCoverageComplete<'_>> {
        let request = self.standard_bag_request.as_ref()?;
        (self.pattern_count != 0 && self.complete.load(Ordering::Acquire)).then_some(
            SharedStandardBagCoverageComplete {
                request,
                pattern_count: self.pattern_count,
            },
        )
    }

    #[cfg(feature = "placement-dependency-pc")]
    fn validate_request(
        &self,
        request: Option<&Arc<SharedStandardBagRequest>>,
    ) -> Result<(), WasmExactSearchError> {
        if let Some(bound) = &self.standard_bag_request {
            if !request.is_some_and(|request| Arc::ptr_eq(bound, request)) {
                return Err(WasmExactSearchError::InvalidProblem(
                    "wasm_parallel_coverage_request_mismatch",
                ));
            }
        }
        Ok(())
    }

    #[cfg(feature = "placement-dependency-pc")]
    pub(super) fn union_for_request(
        &self,
        request: Option<&Arc<SharedStandardBagRequest>>,
        coverage: &PatternBitSet,
    ) -> Result<(), WasmExactSearchError> {
        self.validate_request(request)?;
        self.union_checked(coverage)
    }

    #[cfg(any(test, not(feature = "placement-dependency-pc")))]
    pub fn union(&self, coverage: &PatternBitSet) -> Result<(), WasmExactSearchError> {
        #[cfg(feature = "placement-dependency-pc")]
        self.validate_request(None)?;
        self.union_checked(coverage)
    }

    fn union_checked(&self, coverage: &PatternBitSet) -> Result<(), WasmExactSearchError> {
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
    #[cfg(any(test, not(feature = "placement-dependency-pc")))]
    pub(super) fn union_original_words(
        &self,
        pattern_count: usize,
        coverage: Option<&[u64]>,
    ) -> Result<(), WasmExactSearchError> {
        #[cfg(feature = "placement-dependency-pc")]
        self.validate_request(None)?;
        self.union_original_words_checked(pattern_count, coverage)
    }

    #[cfg(feature = "placement-dependency-pc")]
    pub(super) fn union_original_words_for_request(
        &self,
        request: Option<&Arc<SharedStandardBagRequest>>,
        pattern_count: usize,
        coverage: Option<&[u64]>,
    ) -> Result<(), WasmExactSearchError> {
        self.validate_request(request)?;
        self.union_original_words_checked(pattern_count, coverage)
    }

    fn union_original_words_checked(
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
        if required.pattern_count() != self.pattern_count {
            return false;
        }
        // A published exact full bitmap absorbs every set in this same
        // universe. Keep the count check before the O(1) monotone latch.
        self.complete.load(Ordering::Acquire)
            || self
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
    fn complete_superset_latch_still_checks_universe_and_partial_bits() {
        let coverage = SharedCoverage::new(129);
        let present = PatternBitSet::from_words(129, vec![1, 2, 1]).unwrap();
        let absent = PatternBitSet::from_words(129, vec![2, 0, 0]).unwrap();
        coverage.union(&present).unwrap();
        assert!(!coverage.complete.load(Ordering::Acquire));
        assert!(coverage.is_superset(&present));
        assert!(!coverage.is_superset(&absent));
        assert!(!coverage.is_superset(&PatternBitSet::new(128)));
        coverage.union(&PatternBitSet::all(129)).unwrap();
        assert!(coverage.complete.load(Ordering::Acquire));
        assert!(coverage.is_superset(&absent));
        assert!(coverage.is_superset(&PatternBitSet::all(129)));
        assert!(!coverage.is_superset(&PatternBitSet::new(128)));
        assert!(!coverage.is_superset(&PatternBitSet::new(130)));
    }

    #[cfg(feature = "placement-dependency-pc")]
    #[test]
    fn bound_publication_rejects_foreign_owner_before_writes_and_requires_exact_nonempty_full_set()
    {
        let request = Arc::new(SharedStandardBagRequest::default());
        let foreign = Arc::new(SharedStandardBagRequest::default());
        let shared = SharedCoverage::for_standard_bag_request(129, Arc::clone(&request));
        assert!(shared.standard_bag_complete_proof().is_none());
        assert!(shared.union(&PatternBitSet::all(129)).is_err());
        assert!(shared
            .union_for_request(Some(&foreign), &PatternBitSet::all(129))
            .is_err());
        assert!(shared
            .union_original_words(129, Some(&[u64::MAX; 3]))
            .is_err());
        assert!(shared
            .union_original_words_for_request(Some(&foreign), 129, Some(&[u64::MAX; 3]))
            .is_err());
        assert!(shared
            .union_original_words_for_request(Some(&request), 128, Some(&[u64::MAX; 2]))
            .is_err());
        assert!(shared
            .union_original_words_for_request(Some(&request), 129, Some(&[u64::MAX; 2]))
            .is_err());
        assert!(shared.to_bitset().unwrap().is_empty());
        shared
            .union_original_words_for_request(Some(&request), 129, Some(&[u64::MAX, u64::MAX, 0]))
            .unwrap();
        assert!(shared.standard_bag_complete_proof().is_none());
        // Invalid high tail bits are masked; only the real 129th ordinal can
        // complete this universe, regardless of unused storage bits.
        shared
            .union_original_words_for_request(Some(&request), 129, Some(&[0, 0, !1]))
            .unwrap();
        assert!(shared.standard_bag_complete_proof().is_none());
        shared
            .union_original_words_for_request(Some(&request), 129, Some(&[0, 0, u64::MAX]))
            .unwrap();
        let proof = shared.standard_bag_complete_proof().unwrap();
        assert!(Arc::ptr_eq(proof.request(), &request));
        assert_eq!(proof.pattern_count(), 129);
        assert_eq!(shared.to_bitset().unwrap(), PatternBitSet::all(129));
        let unbound = SharedCoverage::new(129);
        unbound.union(&PatternBitSet::all(129)).unwrap();
        assert!(unbound.standard_bag_complete_proof().is_none());
        let empty = SharedCoverage::for_standard_bag_request(0, request);
        assert!(empty.standard_bag_complete_proof().is_none());
    }

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
