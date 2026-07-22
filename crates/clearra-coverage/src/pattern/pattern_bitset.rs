use std::sync::{Arc, OnceLock};

use super::pattern_id::PatternId;

#[derive(Clone, Debug)]
enum PatternBitStorage {
    Dense(Arc<[u64]>),
    Sparse(Arc<SparsePatternStorage>),
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
            _ => self.words() == other.words(),
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

    pub fn shared_words(&self) -> Arc<[u64]> {
        match &self.storage {
            PatternBitStorage::Dense(words) => Arc::clone(words),
            PatternBitStorage::Sparse(storage) => Arc::clone(storage.words(self.pattern_count)),
        }
    }

    pub fn retained_bytes(&self) -> usize {
        match &self.storage {
            PatternBitStorage::Dense(words) => {
                words.len().saturating_mul(core::mem::size_of::<u64>())
            }
            PatternBitStorage::Sparse(storage) => storage
                .pattern_ids
                .len()
                .saturating_mul(core::mem::size_of::<u32>())
                .saturating_add(storage.dense_words.get().map_or(0, |words| {
                    words.len().saturating_mul(core::mem::size_of::<u64>())
                })),
        }
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
