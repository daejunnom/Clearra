// SRP rationale: this module has one change reason: canonical bounded bit-set operations over pattern identities.
use std::sync::{Arc, OnceLock};

use super::pattern_id::PatternId;

#[derive(Clone, Debug)]
enum PatternBitStorage {
    Dense(Arc<[u64]>),
    Sparse(Arc<SparsePatternStorage>),
}

/// The allocation layout selected by the same byte comparison used by
/// [`PatternBitSet::from_pattern_indices`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatternBitSetStorageLayout {
    Dense,
    Sparse,
}

/// Allocation-free, checked sizing for constructing one pattern bitset.
///
/// The byte counts cover the private Rust values and their element payloads.
/// Allocator metadata is deliberately excluded, consistently with the rest of
/// the execution resource accounting contract. `constructor_peak_bytes`
/// conservatively includes the caller-provided `Vec<u32>` while conversion to
/// the selected `Arc` storage is in flight. `shared_retained_bytes` additionally
/// includes the `PatternBitSet` value owned by an `Arc<PatternBitSet>`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PatternBitSetAllocationProjection {
    pub layout: PatternBitSetStorageLayout,
    pub input_pattern_id_bytes: u128,
    pub storage_retained_bytes: u128,
    pub constructor_peak_bytes: u128,
    pub shared_retained_bytes: u128,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatternBitSetAllocationError {
    InvalidPattern(PatternBitSetError),
    ProjectionOverflow,
    MemoryCapacityExceeded {
        required_memory_bytes: u128,
        max_memory_bytes: u128,
    },
}

#[derive(Debug)]
struct SparsePatternStorage {
    pattern_ids: Arc<[u32]>,
    dense_words: OnceLock<Arc<[u64]>>,
}

impl SparsePatternStorage {
    fn new(pattern_ids: Vec<u32>) -> Self {
        Self {
            pattern_ids: pattern_ids.into(),
            dense_words: OnceLock::new(),
        }
    }

    fn words(&self, pattern_count: usize) -> &Arc<[u64]> {
        self.dense_words.get_or_init(|| {
            let mut words = vec![0_u64; pattern_count.div_ceil(u64::BITS as usize)];
            for pattern_id in self.pattern_ids.iter().copied() {
                let index = pattern_id as usize;
                words[index / u64::BITS as usize] |= 1_u64 << (index % u64::BITS as usize);
            }
            words.into()
        })
    }
}

#[derive(Clone, Debug)]
pub struct PatternBitSet {
    pattern_count: usize,
    storage: PatternBitStorage,
}

/// Opaque identity and byte size for one independently allocated storage
/// component. Equality is pointer identity, never content equality. The
/// address remains private so this runtime accounting token cannot become a
/// serialized or deterministic product identity.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct PatternBitSetStorageComponent {
    address: usize,
    retained_bytes: u128,
}

impl PatternBitSetStorageComponent {
    pub const fn retained_bytes(self) -> u128 {
        self.retained_bytes
    }
}

impl core::fmt::Debug for PatternBitSetStorageComponent {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("PatternBitSetStorageComponent")
            .field("retained_bytes", &self.retained_bytes)
            .finish_non_exhaustive()
    }
}

pub struct CoveredPatternIter<'a> {
    storage: CoveredPatternIterStorage<'a>,
    end_exclusive: usize,
}

enum CoveredPatternIterStorage<'a> {
    Dense {
        words: &'a [u64],
        word_index: usize,
        remaining_word: u64,
    },
    Sparse {
        pattern_ids: &'a [u32],
        index: usize,
    },
}

impl Iterator for CoveredPatternIter<'_> {
    type Item = PatternId;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.storage {
            CoveredPatternIterStorage::Sparse { pattern_ids, index } => {
                let pattern_id = *pattern_ids.get(*index)? as usize;
                if pattern_id >= self.end_exclusive {
                    return None;
                }
                *index += 1;
                Some(PatternId::new(pattern_id))
            }
            CoveredPatternIterStorage::Dense {
                words,
                word_index,
                remaining_word,
            } => loop {
                if *remaining_word != 0 {
                    let bit = remaining_word.trailing_zeros() as usize;
                    *remaining_word &= *remaining_word - 1;
                    let pattern_id = *word_index * u64::BITS as usize + bit;
                    return (pattern_id < self.end_exclusive).then(|| PatternId::new(pattern_id));
                }
                *word_index += 1;
                if *word_index * u64::BITS as usize >= self.end_exclusive {
                    return None;
                }
                *remaining_word = *words.get(*word_index)?;
            },
        }
    }
}

impl PartialEq for PatternBitSet {
    fn eq(&self, other: &Self) -> bool {
        if self.pattern_count != other.pattern_count {
            return false;
        }
        match (&self.storage, &other.storage) {
            (PatternBitStorage::Sparse(left), PatternBitStorage::Sparse(right)) => {
                left.pattern_ids == right.pattern_ids
            }
            _ => (0..self.word_count())
                .all(|word_index| self.word_at(word_index) == other.word_at(word_index)),
        }
    }
}

impl Eq for PatternBitSet {}

impl PatternBitSet {
    pub fn new(pattern_count: usize) -> Self {
        Self {
            pattern_count,
            storage: PatternBitStorage::Dense(
                vec![0; pattern_count.div_ceil(u64::BITS as usize)].into(),
            ),
        }
    }

    pub fn all(pattern_count: usize) -> Self {
        let mut words = vec![u64::MAX; pattern_count.div_ceil(u64::BITS as usize)];
        if let (Some(last), remainder) = (words.last_mut(), pattern_count % u64::BITS as usize) {
            if remainder != 0 {
                *last = (1_u64 << remainder) - 1;
            }
        }
        Self {
            pattern_count,
            storage: PatternBitStorage::Dense(words.into()),
        }
    }

    /// Constructs an all-pattern set only after its worst construction peak is
    /// proven to fit the same memory cap that governs the surrounding search.
    pub fn all_with_memory_limit(
        pattern_count: usize,
        already_retained_bytes: u128,
        max_memory_bytes: u128,
    ) -> Result<Self, PatternBitSetAllocationError> {
        let projection = Self::checked_all_projection(pattern_count)
            .ok_or(PatternBitSetAllocationError::ProjectionOverflow)?;
        ensure_projection_fits(
            already_retained_bytes,
            projection.constructor_peak_bytes,
            max_memory_bytes,
        )?;
        Ok(Self::all(pattern_count))
    }

    /// Sizes the dense all-pattern constructor, including the temporary
    /// `Vec<u64>` and final `Arc<[u64]>` payloads that can coexist during
    /// conversion.
    pub fn checked_all_projection(
        pattern_count: usize,
    ) -> Option<PatternBitSetAllocationProjection> {
        let dense_word_count = checked_dense_word_count(pattern_count)? as u128;
        let dense_bytes = dense_word_count.checked_mul(core::mem::size_of::<u64>() as u128)?;
        let shared_retained_bytes =
            (core::mem::size_of::<PatternBitSet>() as u128).checked_add(dense_bytes)?;
        Some(PatternBitSetAllocationProjection {
            layout: PatternBitSetStorageLayout::Dense,
            input_pattern_id_bytes: 0,
            storage_retained_bytes: dense_bytes,
            constructor_peak_bytes: dense_bytes.checked_mul(2)?.max(shared_retained_bytes),
            shared_retained_bytes,
        })
    }

    /// Conservatively sizes materializing one dense word vector into a
    /// `PatternBitSet` and unioning it into an arbitrary existing set.
    ///
    /// The bound follows every representation branch rather than assuming the
    /// final storage is dense. A sparse/sparse union can temporarily own two
    /// merged-ID payloads, each strictly smaller than two dense payloads, while
    /// the source set and sparse metadata remain live. The six dense payloads
    /// cover the caller-owned source words, the materialized source set, and
    /// those four merge payloads; the two metadata values cover the source and
    /// replacement sparse owners. Allocator metadata is excluded under the
    /// same accounting contract as the other projection helpers in this type.
    pub fn checked_external_words_materialize_union_future_bytes(
        pattern_count: usize,
    ) -> Option<u128> {
        let dense_word_count = checked_dense_word_count(pattern_count)? as u128;
        let dense_bytes = dense_word_count.checked_mul(core::mem::size_of::<u64>() as u128)?;
        if dense_bytes == 0 {
            return Some(0);
        }
        dense_bytes
            .checked_mul(6)?
            .checked_add((core::mem::size_of::<SparsePatternStorage>() as u128).checked_mul(2)?)
    }

    pub fn from_words(
        pattern_count: usize,
        mut words: Vec<u64>,
    ) -> Result<Self, PatternBitSetError> {
        let expected_word_count = pattern_count.div_ceil(u64::BITS as usize);
        if words.len() != expected_word_count {
            return Err(PatternBitSetError::WordCountMismatch {
                expected: expected_word_count,
                actual: words.len(),
            });
        }
        if let (Some(last), remainder) = (words.last_mut(), pattern_count % u64::BITS as usize) {
            if remainder != 0 {
                *last &= (1_u64 << remainder) - 1;
            }
        }
        Ok(Self {
            pattern_count,
            storage: compact_storage_from_words(pattern_count, words),
        })
    }

    pub fn from_shared_words(
        pattern_count: usize,
        words: Arc<[u64]>,
    ) -> Result<Self, PatternBitSetError> {
        let expected_word_count = pattern_count.div_ceil(u64::BITS as usize);
        if words.len() != expected_word_count {
            return Err(PatternBitSetError::WordCountMismatch {
                expected: expected_word_count,
                actual: words.len(),
            });
        }
        let remainder = pattern_count % u64::BITS as usize;
        if remainder != 0
            && words
                .last()
                .is_some_and(|last| last & !((1_u64 << remainder) - 1) != 0)
        {
            let mut owned = words.as_ref().to_vec();
            if let Some(last) = owned.last_mut() {
                *last &= (1_u64 << remainder) - 1;
            }
            return Ok(Self {
                pattern_count,
                storage: compact_storage_from_words(pattern_count, owned),
            });
        }
        Ok(Self {
            pattern_count,
            storage: PatternBitStorage::Dense(words),
        })
    }

    pub fn from_pattern_indices(
        pattern_count: usize,
        mut pattern_ids: Vec<u32>,
    ) -> Result<Self, PatternBitSetError> {
        pattern_ids.sort_unstable();
        pattern_ids.dedup();
        if let Some(index) = pattern_ids
            .last()
            .copied()
            .map(|index| index as usize)
            .filter(|index| *index >= pattern_count)
        {
            return Err(PatternBitSetError::PatternOutOfRange {
                index,
                pattern_count,
            });
        }
        Ok(Self {
            pattern_count,
            storage: compact_storage_from_pattern_ids(pattern_count, pattern_ids),
        })
    }

    /// Sorts and validates the supplied IDs without allocating, then checks the
    /// complete conversion peak before the first storage allocation.
    pub fn from_pattern_indices_with_memory_limit(
        pattern_count: usize,
        mut pattern_ids: Vec<u32>,
        already_retained_bytes: u128,
        max_memory_bytes: u128,
    ) -> Result<Self, PatternBitSetAllocationError> {
        pattern_ids.sort_unstable();
        pattern_ids.dedup();
        if let Some(index) = pattern_ids
            .last()
            .copied()
            .map(|index| index as usize)
            .filter(|index| *index >= pattern_count)
        {
            return Err(PatternBitSetAllocationError::InvalidPattern(
                PatternBitSetError::PatternOutOfRange {
                    index,
                    pattern_count,
                },
            ));
        }
        let projection = Self::checked_allocation_projection(
            pattern_count,
            pattern_ids.len(),
            pattern_ids.capacity(),
        )
        .ok_or(PatternBitSetAllocationError::ProjectionOverflow)?;
        ensure_projection_fits(
            already_retained_bytes,
            projection.constructor_peak_bytes,
            max_memory_bytes,
        )?;
        Ok(Self {
            pattern_count,
            storage: compact_storage_from_pattern_ids(pattern_count, pattern_ids),
        })
    }

    /// Computes the exact private layout decision and a conservative conversion
    /// peak without allocating. `unique_pattern_count` is the post-dedup length;
    /// `input_capacity` is the live input `Vec<u32>` capacity.
    pub fn checked_allocation_projection(
        pattern_count: usize,
        unique_pattern_count: usize,
        input_capacity: usize,
    ) -> Option<PatternBitSetAllocationProjection> {
        if unique_pattern_count > input_capacity || unique_pattern_count > pattern_count {
            return None;
        }
        let dense_word_count = checked_dense_word_count(pattern_count)?;
        let dense_bytes =
            (dense_word_count as u128).checked_mul(core::mem::size_of::<u64>() as u128)?;
        let sparse_payload_bytes =
            (unique_pattern_count as u128).checked_mul(core::mem::size_of::<u32>() as u128)?;
        let input_pattern_id_bytes =
            (input_capacity as u128).checked_mul(core::mem::size_of::<u32>() as u128)?;
        let sparse_owner_bytes = core::mem::size_of::<SparsePatternStorage>() as u128;
        let pattern_bitset_owner_bytes = core::mem::size_of::<PatternBitSet>() as u128;

        let (layout, storage_retained_bytes, constructor_peak_bytes) =
            if sparse_payload_bytes < dense_bytes {
                let retained = sparse_owner_bytes.checked_add(sparse_payload_bytes)?;
                let peak = input_pattern_id_bytes.checked_add(retained)?;
                (PatternBitSetStorageLayout::Sparse, retained, peak)
            } else {
                // Vec<u64> -> Arc<[u64]> may hold both payloads while the input
                // pattern-id Vec remains live, so count both dense payloads.
                let peak = input_pattern_id_bytes.checked_add(dense_bytes.checked_mul(2)?)?;
                (PatternBitSetStorageLayout::Dense, dense_bytes, peak)
            };
        let shared_retained_bytes =
            pattern_bitset_owner_bytes.checked_add(storage_retained_bytes)?;
        Some(PatternBitSetAllocationProjection {
            layout,
            input_pattern_id_bytes,
            storage_retained_bytes,
            constructor_peak_bytes: constructor_peak_bytes.max(shared_retained_bytes),
            shared_retained_bytes,
        })
    }

    /// Bounds the retained storage of `group_count` independently shared
    /// bitsets containing at most `total_pattern_ids` IDs in aggregate. This is
    /// safe for any mixture of the private dense and sparse layouts.
    pub fn checked_shared_storage_upper_bound(
        pattern_count: usize,
        group_count: u128,
        total_pattern_ids: u128,
    ) -> Option<u128> {
        let dense_word_count = checked_dense_word_count(pattern_count)? as u128;
        let dense_bytes = dense_word_count.checked_mul(core::mem::size_of::<u64>() as u128)?;
        let fixed_per_group = (core::mem::size_of::<PatternBitSet>() as u128)
            .checked_add(dense_bytes.max(core::mem::size_of::<SparsePatternStorage>() as u128))?;
        group_count
            .checked_mul(fixed_per_group)?
            .checked_add(total_pattern_ids.checked_mul(core::mem::size_of::<u32>() as u128)?)
    }

    /// Bounds the peak while independently shared bitsets are built from live
    /// pattern-ID vectors. Dense conversion can temporarily own two word
    /// payloads; sparse conversion can temporarily own both input and Arc ID
    /// payloads.
    pub fn checked_shared_construction_upper_bound(
        pattern_count: usize,
        group_count: u128,
        total_pattern_ids: u128,
    ) -> Option<u128> {
        let dense_word_count = checked_dense_word_count(pattern_count)? as u128;
        let dense_conversion_bytes = dense_word_count
            .checked_mul(core::mem::size_of::<u64>() as u128)?
            .checked_mul(2)?;
        let sparse_fixed_bytes = core::mem::size_of::<SparsePatternStorage>() as u128;
        let pattern_bitset_owner_bytes = core::mem::size_of::<PatternBitSet>() as u128;
        let fixed_per_group = pattern_bitset_owner_bytes
            .checked_add(dense_conversion_bytes.max(sparse_fixed_bytes))?;
        group_count.checked_mul(fixed_per_group)?.checked_add(
            total_pattern_ids
                .checked_mul(core::mem::size_of::<u32>() as u128)?
                .checked_mul(2)?,
        )
    }
}

fn checked_dense_word_count(pattern_count: usize) -> Option<usize> {
    let whole_words = pattern_count / u64::BITS as usize;
    whole_words.checked_add(usize::from(pattern_count % u64::BITS as usize != 0))
}

fn ensure_projection_fits(
    already_retained_bytes: u128,
    constructor_peak_bytes: u128,
    max_memory_bytes: u128,
) -> Result<(), PatternBitSetAllocationError> {
    let required_memory_bytes = already_retained_bytes
        .checked_add(constructor_peak_bytes)
        .ok_or(PatternBitSetAllocationError::ProjectionOverflow)?;
    if required_memory_bytes <= max_memory_bytes {
        return Ok(());
    }
    Err(PatternBitSetAllocationError::MemoryCapacityExceeded {
        required_memory_bytes,
        max_memory_bytes,
    })
}

impl PatternBitSet {
    pub fn new_with_word_budget(
        pattern_count: usize,
        word_limit: usize,
    ) -> Result<Self, PatternBitSetError> {
        let word_count = pattern_count.div_ceil(u64::BITS as usize);
        if word_count > word_limit {
            return Err(PatternBitSetError::WordCapacityExceeded {
                word_count,
                word_limit,
            });
        }
        Ok(Self::new(pattern_count))
    }

    pub fn from_patterns(
        pattern_count: usize,
        patterns: impl IntoIterator<Item = PatternId>,
    ) -> Result<Self, PatternBitSetError> {
        let patterns = patterns.into_iter().collect::<Vec<_>>();
        if patterns
            .iter()
            .any(|pattern| pattern.index() > u32::MAX as usize)
        {
            let mut bitset = Self::new(pattern_count);
            for pattern in patterns {
                bitset.insert(pattern)?;
            }
            return Ok(bitset);
        }
        Self::from_pattern_indices(
            pattern_count,
            patterns
                .into_iter()
                .map(|pattern| pattern.index() as u32)
                .collect(),
        )
    }

    pub const fn pattern_count(&self) -> usize {
        self.pattern_count
    }

    pub fn word_count(&self) -> usize {
        self.pattern_count.div_ceil(u64::BITS as usize)
    }

    pub fn words(&self) -> &[u64] {
        match &self.storage {
            PatternBitStorage::Dense(words) => words,
            PatternBitStorage::Sparse(storage) => storage.words(self.pattern_count),
        }
    }

    /// Returns one canonical dense word without materializing the sparse
    /// storage's lazy dense cache. Validation and streaming callers can use
    /// this accessor without introducing a count-proportional allocation.
    pub fn word_at(&self, word_index: usize) -> u64 {
        let Some(first_pattern) = word_index.checked_mul(u64::BITS as usize) else {
            return 0;
        };
        if first_pattern >= self.pattern_count {
            return 0;
        }
        match &self.storage {
            PatternBitStorage::Dense(words) => words.get(word_index).copied().unwrap_or(0),
            PatternBitStorage::Sparse(storage) => {
                let last_pattern = first_pattern
                    .saturating_add(u64::BITS as usize)
                    .min(self.pattern_count);
                let first = storage
                    .pattern_ids
                    .partition_point(|pattern| (*pattern as usize) < first_pattern);
                let mut word = 0_u64;
                for pattern in storage.pattern_ids[first..]
                    .iter()
                    .copied()
                    .take_while(|pattern| (*pattern as usize) < last_pattern)
                {
                    word |= 1_u64 << (pattern as usize - first_pattern);
                }
                word
            }
        }
    }

    /// Copies the canonical dense representation without populating a sparse
    /// bitset's lazy dense cache. The returned vector is the sole new owner.
    pub fn to_owned_words(&self) -> Vec<u64> {
        (0..self.word_count())
            .map(|word_index| self.word_at(word_index))
            .collect()
    }

    pub fn shared_words(&self) -> Arc<[u64]> {
        match &self.storage {
            PatternBitStorage::Dense(words) => Arc::clone(words),
            PatternBitStorage::Sparse(storage) => Arc::clone(storage.words(self.pattern_count)),
        }
    }

    pub fn retained_bytes(&self) -> usize {
        usize::try_from(self.checked_storage_retained_bytes().unwrap_or(u128::MAX))
            .unwrap_or(usize::MAX)
    }

    /// Checked heap backing for this exact storage identity, excluding the
    /// inline `PatternBitSet` value. Sparse storage includes the heap-owned
    /// `SparsePatternStorage` value itself as well as both Arc slice payloads.
    pub fn checked_storage_retained_bytes(&self) -> Option<u128> {
        let mut bytes = 0_u128;
        for index in 0..self.storage_component_count() {
            bytes = bytes.checked_add(self.storage_component(index)?.retained_bytes())?;
        }
        Some(bytes)
    }

    pub fn storage_component_count(&self) -> usize {
        match &self.storage {
            PatternBitStorage::Dense(words) => usize::from(!words.is_empty()),
            PatternBitStorage::Sparse(storage) => {
                1 + usize::from(!storage.pattern_ids.is_empty())
                    + usize::from(
                        storage
                            .dense_words
                            .get()
                            .is_some_and(|words| !words.is_empty()),
                    )
            }
        }
    }

    /// Returns one opaque allocation component. A sparse bitset exposes its
    /// owner value, pattern-id slice, and optional dense cache separately; this
    /// lets consumers deduplicate a cache shared with a promoted dense clone.
    pub fn storage_component(&self, index: usize) -> Option<PatternBitSetStorageComponent> {
        match &self.storage {
            PatternBitStorage::Dense(words) => {
                (index == 0 && !words.is_empty()).then(|| PatternBitSetStorageComponent {
                    address: words.as_ptr() as usize,
                    retained_bytes: words.len() as u128 * core::mem::size_of::<u64>() as u128,
                })
            }
            PatternBitStorage::Sparse(storage) => {
                let mut remaining = index;
                if remaining == 0 {
                    return Some(PatternBitSetStorageComponent {
                        address: Arc::as_ptr(storage) as usize,
                        retained_bytes: core::mem::size_of::<SparsePatternStorage>() as u128,
                    });
                }
                remaining -= 1;
                if !storage.pattern_ids.is_empty() {
                    if remaining == 0 {
                        return Some(PatternBitSetStorageComponent {
                            address: storage.pattern_ids.as_ptr() as usize,
                            retained_bytes: storage.pattern_ids.len() as u128
                                * core::mem::size_of::<u32>() as u128,
                        });
                    }
                    remaining -= 1;
                }
                storage
                    .dense_words
                    .get()
                    .filter(|words| !words.is_empty() && remaining == 0)
                    .map(|words| PatternBitSetStorageComponent {
                        address: words.as_ptr() as usize,
                        retained_bytes: words.len() as u128 * core::mem::size_of::<u64>() as u128,
                    })
            }
        }
    }

    /// Pointer identity for exact allocation accounting. Content-equal but
    /// independently allocated bitsets are deliberately not considered shared.
    pub fn shares_storage_with(&self, other: &Self) -> bool {
        match (&self.storage, &other.storage) {
            (PatternBitStorage::Dense(left), PatternBitStorage::Dense(right)) => {
                Arc::ptr_eq(left, right)
            }
            (PatternBitStorage::Sparse(left), PatternBitStorage::Sparse(right)) => {
                Arc::ptr_eq(left, right)
            }
            _ => false,
        }
    }

    /// Checked private storage plus the `PatternBitSet` value owned by one
    /// `Arc<PatternBitSet>`. Shared callers must count a pointer-identical value
    /// only once.
    pub fn checked_shared_retained_bytes(&self) -> Option<u128> {
        (core::mem::size_of::<PatternBitSet>() as u128)
            .checked_add(self.checked_storage_retained_bytes()?)
    }
}

impl PatternBitSet {
    pub fn insert(&mut self, pattern: PatternId) -> Result<(), PatternBitSetError> {
        let index = pattern.index();
        if index >= self.pattern_count {
            return Err(PatternBitSetError::PatternOutOfRange {
                index,
                pattern_count: self.pattern_count,
            });
        }
        self.dense_words_mut()[index / u64::BITS as usize] |= 1_u64 << (index % u64::BITS as usize);
        Ok(())
    }

    pub fn contains(&self, pattern: PatternId) -> bool {
        let index = pattern.index();
        if index >= self.pattern_count {
            return false;
        }
        match &self.storage {
            PatternBitStorage::Dense(words) => {
                words[index / u64::BITS as usize] & (1_u64 << (index % u64::BITS as usize)) != 0
            }
            PatternBitStorage::Sparse(storage) => u32::try_from(index)
                .ok()
                .is_some_and(|index| storage.pattern_ids.binary_search(&index).is_ok()),
        }
    }

    pub fn union(&self, other: &Self) -> Result<Self, PatternBitSetError> {
        let mut union = self.clone();
        union.union_with(other)?;
        Ok(union)
    }

    pub fn union_with(&mut self, other: &Self) -> Result<(), PatternBitSetError> {
        if self.pattern_count != other.pattern_count {
            return Err(PatternBitSetError::PatternUniverseMismatch {
                left: self.pattern_count,
                right: other.pattern_count,
            });
        }
        if let (PatternBitStorage::Sparse(left), PatternBitStorage::Sparse(right)) =
            (&self.storage, &other.storage)
        {
            let mut merged = Vec::with_capacity(
                left.pattern_ids
                    .len()
                    .saturating_add(right.pattern_ids.len()),
            );
            merge_pattern_ids(&left.pattern_ids, &right.pattern_ids, &mut merged);
            self.storage = compact_storage_from_pattern_ids(self.pattern_count, merged);
            return Ok(());
        }
        if let PatternBitStorage::Sparse(right) = &other.storage {
            let left = self.dense_words_mut();
            for pattern_id in right.pattern_ids.iter().copied() {
                let index = pattern_id as usize;
                left[index / u64::BITS as usize] |= 1_u64 << (index % u64::BITS as usize);
            }
            return Ok(());
        }
        let right = other.words();
        for (left, right) in self.dense_words_mut().iter_mut().zip(right.iter()) {
            *left |= right;
        }
        Ok(())
    }

    pub fn is_superset(&self, required: &Self) -> Result<bool, PatternBitSetError> {
        if self.pattern_count != required.pattern_count {
            return Err(PatternBitSetError::PatternUniverseMismatch {
                left: self.pattern_count,
                right: required.pattern_count,
            });
        }
        if let PatternBitStorage::Sparse(required) = &required.storage {
            return Ok(required
                .pattern_ids
                .iter()
                .copied()
                .all(|pattern| self.contains(PatternId::new(pattern as usize))));
        }
        Ok(required
            .words()
            .iter()
            .enumerate()
            .all(|(index, required_word)| {
                let current = self.words().get(index).copied().unwrap_or(0);
                current & required_word == *required_word
            }))
    }

    pub fn is_empty(&self) -> bool {
        match &self.storage {
            PatternBitStorage::Dense(words) => words.iter().all(|word| *word == 0),
            PatternBitStorage::Sparse(storage) => storage.pattern_ids.is_empty(),
        }
    }

    pub fn count_ones(&self) -> u32 {
        match &self.storage {
            PatternBitStorage::Dense(words) => words
                .iter()
                .fold(0_u32, |count, word| count.saturating_add(word.count_ones())),
            PatternBitStorage::Sparse(storage) => {
                u32::try_from(storage.pattern_ids.len()).unwrap_or(u32::MAX)
            }
        }
    }

    pub fn covered_patterns(&self) -> Vec<PatternId> {
        self.covered_patterns_before(self.pattern_count).collect()
    }

    pub fn covered_patterns_before(&self, end_exclusive: usize) -> CoveredPatternIter<'_> {
        let end_exclusive = end_exclusive.min(self.pattern_count);
        let storage = match &self.storage {
            PatternBitStorage::Dense(words) => CoveredPatternIterStorage::Dense {
                words,
                word_index: 0,
                remaining_word: words.first().copied().unwrap_or(0),
            },
            PatternBitStorage::Sparse(storage) => CoveredPatternIterStorage::Sparse {
                pattern_ids: &storage.pattern_ids,
                index: 0,
            },
        };
        CoveredPatternIter {
            storage,
            end_exclusive,
        }
    }

    pub fn first_pattern(&self) -> Option<PatternId> {
        match &self.storage {
            PatternBitStorage::Sparse(storage) => storage
                .pattern_ids
                .first()
                .copied()
                .map(|pattern| PatternId::new(pattern as usize)),
            PatternBitStorage::Dense(words) => {
                words.iter().enumerate().find_map(|(word_index, word)| {
                    (*word != 0).then(|| {
                        PatternId::new(
                            word_index * u64::BITS as usize + word.trailing_zeros() as usize,
                        )
                    })
                })
            }
        }
    }

    fn dense_words_mut(&mut self) -> &mut [u64] {
        if let PatternBitStorage::Sparse(storage) = &self.storage {
            self.storage = PatternBitStorage::Dense(Arc::clone(storage.words(self.pattern_count)));
        }
        match &mut self.storage {
            PatternBitStorage::Dense(words) => Arc::make_mut(words),
            PatternBitStorage::Sparse(_) => unreachable!("sparse storage was materialized"),
        }
    }
}

fn compact_storage_from_words(pattern_count: usize, words: Vec<u64>) -> PatternBitStorage {
    let covered_count = words
        .iter()
        .map(|word| word.count_ones() as usize)
        .sum::<usize>();
    let sparse_bytes = covered_count.saturating_mul(core::mem::size_of::<u32>());
    let dense_bytes = words.len().saturating_mul(core::mem::size_of::<u64>());
    if pattern_count as u128 > u32::MAX as u128 + 1 || sparse_bytes >= dense_bytes {
        return PatternBitStorage::Dense(words.into());
    }
    let mut pattern_ids = Vec::with_capacity(covered_count);
    for (word_index, source_word) in words.into_iter().enumerate() {
        let mut word = source_word;
        while word != 0 {
            let bit = word.trailing_zeros() as usize;
            word &= word - 1;
            pattern_ids.push((word_index * u64::BITS as usize + bit) as u32);
        }
    }
    PatternBitStorage::Sparse(Arc::new(SparsePatternStorage::new(pattern_ids)))
}

fn compact_storage_from_pattern_ids(
    pattern_count: usize,
    pattern_ids: Vec<u32>,
) -> PatternBitStorage {
    let dense_word_count = pattern_count.div_ceil(u64::BITS as usize);
    let sparse_bytes = pattern_ids
        .len()
        .saturating_mul(core::mem::size_of::<u32>());
    let dense_bytes = dense_word_count.saturating_mul(core::mem::size_of::<u64>());
    if sparse_bytes < dense_bytes {
        return PatternBitStorage::Sparse(Arc::new(SparsePatternStorage::new(pattern_ids)));
    }
    let mut words = vec![0_u64; dense_word_count];
    for pattern_id in pattern_ids {
        let index = pattern_id as usize;
        words[index / u64::BITS as usize] |= 1_u64 << (index % u64::BITS as usize);
    }
    PatternBitStorage::Dense(words.into())
}

fn merge_pattern_ids(left: &[u32], right: &[u32], out: &mut Vec<u32>) {
    let mut left_index = 0usize;
    let mut right_index = 0usize;
    while left_index < left.len() || right_index < right.len() {
        let next = match (left.get(left_index), right.get(right_index)) {
            (Some(left), Some(right)) if left < right => {
                left_index += 1;
                *left
            }
            (Some(left), Some(right)) if right < left => {
                right_index += 1;
                *right
            }
            (Some(left), Some(_)) => {
                left_index += 1;
                right_index += 1;
                *left
            }
            (Some(left), None) => {
                left_index += 1;
                *left
            }
            (None, Some(right)) => {
                right_index += 1;
                *right
            }
            (None, None) => break,
        };
        out.push(next);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatternBitSetError {
    PatternOutOfRange {
        index: usize,
        pattern_count: usize,
    },
    PatternUniverseMismatch {
        left: usize,
        right: usize,
    },
    WordCapacityExceeded {
        word_count: usize,
        word_limit: usize,
    },
    WordCountMismatch {
        expected: usize,
        actual: usize,
    },
}

#[cfg(test)]
#[path = "pattern_bitset_tests.rs"]
mod tests;

#[cfg(test)]
mod allocation_projection_tests {
    use super::*;

    #[test]
    fn projection_uses_the_same_strict_dense_sparse_threshold_as_storage() {
        let sparse = PatternBitSet::checked_allocation_projection(128, 3, 3)
            .expect("three IDs fit the sparse representation");
        let dense = PatternBitSet::checked_allocation_projection(128, 4, 4)
            .expect("four IDs reach the dense representation threshold");

        assert_eq!(sparse.layout, PatternBitSetStorageLayout::Sparse);
        assert_eq!(dense.layout, PatternBitSetStorageLayout::Dense);
        assert!(matches!(
            PatternBitSet::from_pattern_indices(128, vec![0, 1, 2])
                .expect("valid sparse set")
                .storage,
            PatternBitStorage::Sparse(_)
        ));
        assert!(matches!(
            PatternBitSet::from_pattern_indices(128, vec![0, 1, 2, 3])
                .expect("valid dense set")
                .storage,
            PatternBitStorage::Dense(_)
        ));
    }

    #[test]
    fn capped_constructor_rejects_before_storage_allocation_and_accepts_exact_cap() {
        let ids = vec![0, 1, 2];
        let projection = PatternBitSet::checked_allocation_projection(128, 3, ids.capacity())
            .expect("bounded projection");
        assert_eq!(
            PatternBitSet::from_pattern_indices_with_memory_limit(
                128,
                ids.clone(),
                7,
                projection.constructor_peak_bytes + 6,
            ),
            Err(PatternBitSetAllocationError::MemoryCapacityExceeded {
                required_memory_bytes: projection.constructor_peak_bytes + 7,
                max_memory_bytes: projection.constructor_peak_bytes + 6,
            })
        );
        let bounded = PatternBitSet::from_pattern_indices_with_memory_limit(
            128,
            ids,
            7,
            projection.constructor_peak_bytes + 7,
        )
        .expect("exact cap admits construction");
        assert_eq!(bounded.covered_patterns().len(), 3);
    }

    #[test]
    fn shared_storage_bound_checks_u128_overflow() {
        assert_eq!(
            PatternBitSet::checked_shared_storage_upper_bound(usize::MAX, u128::MAX, 1),
            None
        );
        assert!(PatternBitSet::checked_allocation_projection(usize::MAX, 0, 0).is_some());
        assert!(PatternBitSet::checked_allocation_projection(1, 2, 2).is_none());
    }

    #[test]
    fn external_word_materialization_union_projection_covers_sparse_metadata() {
        let dense_bytes = 2 * core::mem::size_of::<u64>() as u128;
        assert_eq!(
            PatternBitSet::checked_external_words_materialize_union_future_bytes(128),
            dense_bytes.checked_mul(6).and_then(|bytes| {
                bytes.checked_add(2 * core::mem::size_of::<SparsePatternStorage>() as u128)
            })
        );
        assert_eq!(
            PatternBitSet::checked_external_words_materialize_union_future_bytes(0),
            Some(0)
        );
    }

    #[test]
    fn mixed_storage_equality_does_not_materialize_the_sparse_dense_cache() {
        let mut dense = PatternBitSet::new(1_024);
        dense
            .insert(PatternId::new(7))
            .expect("the test pattern belongs to the dense set");
        let sparse = PatternBitSet::from_pattern_indices(1_024, vec![7])
            .expect("the test pattern belongs to the sparse set");
        let sparse_component_count = sparse.storage_component_count();
        let sparse_retained_bytes = sparse
            .checked_storage_retained_bytes()
            .expect("checked sparse storage");
        assert_eq!(sparse_component_count, 2);

        assert!(dense == sparse);
        assert!(sparse == dense);
        assert_eq!(sparse.storage_component_count(), sparse_component_count);
        assert_eq!(
            sparse.checked_storage_retained_bytes(),
            Some(sparse_retained_bytes)
        );
    }

    #[test]
    fn storage_identity_and_sparse_owner_projection_are_pointer_exact() {
        let sparse = PatternBitSet::from_pattern_indices(128, vec![3])
            .expect("one pattern uses sparse storage");
        let sparse_clone = sparse.clone();
        let equal_but_distinct =
            PatternBitSet::from_pattern_indices(128, vec![3]).expect("independent sparse storage");
        assert!(sparse.shares_storage_with(&sparse_clone));
        assert!(!sparse.shares_storage_with(&equal_but_distinct));
        assert_eq!(
            sparse.checked_storage_retained_bytes(),
            Some(
                core::mem::size_of::<SparsePatternStorage>() as u128
                    + core::mem::size_of::<u32>() as u128
            )
        );
        assert_eq!(
            sparse.checked_shared_retained_bytes(),
            sparse
                .checked_storage_retained_bytes()
                .and_then(|bytes| bytes.checked_add(core::mem::size_of::<PatternBitSet>() as u128))
        );

        let dense = PatternBitSet::all(128);
        let dense_clone = dense.clone();
        assert!(dense.shares_storage_with(&dense_clone));
        assert_eq!(
            dense.checked_storage_retained_bytes(),
            Some(2 * core::mem::size_of::<u64>() as u128)
        );

        let promoted = PatternBitSet::from_shared_words(128, sparse.shared_words())
            .expect("shared dense cache preserves the pattern universe");
        assert!(!sparse.shares_storage_with(&promoted));
        assert_eq!(sparse.storage_component_count(), 3);
        assert_eq!(promoted.storage_component_count(), 1);
        assert_eq!(sparse.storage_component(2), promoted.storage_component(0));
    }
}
