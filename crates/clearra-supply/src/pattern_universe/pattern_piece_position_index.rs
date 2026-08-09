use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;

use super::materialized_pattern_universe::MaterializedPatternUniverse;

const STANDARD_PIECE_COUNT: usize = PieceKind::STANDARD_TETROMINOES.len();

/// Immutable bit-sliced view of a concrete pattern subset.
///
/// Local bits are dense within one packing multiset group. The global pattern
/// ids are retained separately, so BuildOrder/Hold traversal avoids scanning
/// unrelated multiset groups and can still publish the canonical universe bit
/// ordering without copying queue suffixes into worker state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PatternPiecePositionIndex {
    global_pattern_count: usize,
    local_pattern_ids: Vec<u32>,
    sequence_len: usize,
    word_count: usize,
    position_piece_words: Vec<u64>,
}

impl PatternPiecePositionIndex {
    pub fn compile(
        universe: &MaterializedPatternUniverse,
    ) -> Result<Self, PatternPiecePositionIndexError> {
        let pattern_count = universe.pattern_count();
        let mut pattern_ids = Vec::new();
        pattern_ids
            .try_reserve_exact(pattern_count)
            .map_err(|_| PatternPiecePositionIndexError::StorageUnavailable)?;
        for pattern_id in 0..pattern_count {
            pattern_ids.push(
                u32::try_from(pattern_id)
                    .map_err(|_| PatternPiecePositionIndexError::PatternIdCapacityExceeded)?,
            );
        }
        Self::compile_pattern_ids(universe, pattern_ids)
    }

    pub fn compile_subset_before(
        universe: &MaterializedPatternUniverse,
        patterns: &PatternBitSet,
        end_exclusive: usize,
    ) -> Result<Self, PatternPiecePositionIndexError> {
        if patterns.pattern_count() != universe.pattern_count() {
            return Err(PatternPiecePositionIndexError::UniverseMismatch);
        }
        let limit = end_exclusive.min(universe.pattern_count());
        let mut pattern_ids = Vec::new();
        pattern_ids
            .try_reserve_exact(patterns.count_ones() as usize)
            .map_err(|_| PatternPiecePositionIndexError::StorageUnavailable)?;
        for pattern in patterns.covered_patterns_before(limit) {
            pattern_ids.push(
                u32::try_from(pattern.index())
                    .map_err(|_| PatternPiecePositionIndexError::PatternIdCapacityExceeded)?,
            );
        }
        Self::compile_pattern_ids(universe, pattern_ids)
    }

    fn compile_pattern_ids(
        universe: &MaterializedPatternUniverse,
        local_pattern_ids: Vec<u32>,
    ) -> Result<Self, PatternPiecePositionIndexError> {
        debug_assert!(local_pattern_ids.windows(2).all(|ids| ids[0] < ids[1]));
        let sequence_len = local_pattern_ids
            .iter()
            .map(|pattern_id| universe.sequence_len_at(*pattern_id as usize))
            .max()
            .unwrap_or(0);
        let word_count = local_pattern_ids.len().div_ceil(u64::BITS as usize);
        let storage_len = sequence_len
            .checked_mul(STANDARD_PIECE_COUNT)
            .and_then(|count| count.checked_mul(word_count))
            .ok_or(PatternPiecePositionIndexError::StorageOverflow)?;
        let mut position_piece_words = Vec::new();
        position_piece_words
            .try_reserve_exact(storage_len)
            .map_err(|_| PatternPiecePositionIndexError::StorageUnavailable)?;
        position_piece_words.resize(storage_len, 0);

        let mut sequence = Vec::with_capacity(sequence_len);
        for (local_pattern_index, global_pattern_id) in
            local_pattern_ids.iter().copied().enumerate()
        {
            let word_index = local_pattern_index / u64::BITS as usize;
            let bit = 1_u64 << (local_pattern_index % u64::BITS as usize);
            universe.write_sequence_at(global_pattern_id as usize, &mut sequence);
            for (position, piece) in sequence.iter().copied().enumerate() {
                let piece_index = standard_piece_index(piece);
                let index =
                    ((position * STANDARD_PIECE_COUNT + piece_index) * word_count) + word_index;
                position_piece_words[index] |= bit;
            }
        }

        Ok(Self {
            global_pattern_count: universe.pattern_count(),
            local_pattern_ids,
            sequence_len,
            word_count,
            position_piece_words,
        })
    }

    pub const fn global_pattern_count(&self) -> usize {
        self.global_pattern_count
    }

    pub fn local_pattern_count(&self) -> usize {
        self.local_pattern_ids.len()
    }

    pub const fn sequence_len(&self) -> usize {
        self.sequence_len
    }

    pub const fn word_count(&self) -> usize {
        self.word_count
    }

    pub fn global_pattern_index(&self, local_pattern_index: usize) -> Option<usize> {
        self.local_pattern_ids
            .get(local_pattern_index)
            .copied()
            .map(|pattern_id| pattern_id as usize)
    }

    /// Returns the dense local bit index for a canonical global pattern id.
    pub fn local_pattern_index(&self, global_pattern_index: usize) -> Option<usize> {
        let pattern_id = u32::try_from(global_pattern_index).ok()?;
        self.local_pattern_ids.binary_search(&pattern_id).ok()
    }

    pub fn active_word(&self, word_index: usize) -> u64 {
        if word_index >= self.word_count {
            return 0;
        }
        if word_index + 1 != self.word_count {
            return u64::MAX;
        }
        let remainder = self.local_pattern_ids.len() % u64::BITS as usize;
        if remainder == 0 {
            u64::MAX
        } else {
            (1_u64 << remainder) - 1
        }
    }

    pub fn piece_word(&self, position: usize, piece_code: u8, word_index: usize) -> u64 {
        if position >= self.sequence_len
            || !(1..=STANDARD_PIECE_COUNT as u8).contains(&piece_code)
            || word_index >= self.word_count
        {
            return 0;
        }
        let piece_index = usize::from(piece_code - 1);
        self.position_piece_words
            [((position * STANDARD_PIECE_COUNT + piece_index) * self.word_count) + word_index]
    }

    pub fn piece_word_with_projected_standard_bag_lookahead(
        &self,
        position: usize,
        piece_code: u8,
        word_index: usize,
        projects_standard_bag_lookahead: bool,
    ) -> u64 {
        if position < self.sequence_len || !projects_standard_bag_lookahead {
            return self.piece_word(position, piece_code, word_index);
        }
        if position != self.sequence_len
            || self.sequence_len % STANDARD_PIECE_COUNT != STANDARD_PIECE_COUNT - 1
            || !(1..=STANDARD_PIECE_COUNT as u8).contains(&piece_code)
            || word_index >= self.word_count
        {
            return 0;
        }

        let current_bag_start = self.sequence_len - (STANDARD_PIECE_COUNT - 1);
        let mut present = 0_u64;
        for source_position in current_bag_start..self.sequence_len {
            present |= self.piece_word(source_position, piece_code, word_index);
        }
        self.active_word(word_index) & !present
    }

    pub fn expand_coverage_words(
        &self,
        local_words: &[u64],
    ) -> Result<PatternBitSet, PatternPiecePositionIndexError> {
        if local_words.len() != self.word_count {
            return Err(PatternPiecePositionIndexError::CoverageWordCountMismatch);
        }
        let covered_count = local_words
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum::<usize>();
        let mut global_pattern_ids = Vec::new();
        global_pattern_ids
            .try_reserve_exact(covered_count)
            .map_err(|_| PatternPiecePositionIndexError::StorageUnavailable)?;
        for (word_index, source_word) in local_words.iter().copied().enumerate() {
            let mut word = source_word & self.active_word(word_index);
            while word != 0 {
                let bit = word.trailing_zeros() as usize;
                word &= word - 1;
                let local_pattern_id = word_index * u64::BITS as usize + bit;
                global_pattern_ids.push(self.local_pattern_ids[local_pattern_id]);
            }
        }
        PatternBitSet::from_pattern_indices(self.global_pattern_count, global_pattern_ids)
            .map_err(|_| PatternPiecePositionIndexError::CoverageWordCountMismatch)
    }

    pub fn retained_bytes(&self) -> usize {
        self.position_piece_words
            .capacity()
            .saturating_mul(core::mem::size_of::<u64>())
            .saturating_add(
                self.local_pattern_ids
                    .capacity()
                    .saturating_mul(core::mem::size_of::<u32>()),
            )
    }
}

const fn standard_piece_index(piece: PieceKind) -> usize {
    match piece {
        PieceKind::I => 0,
        PieceKind::O => 1,
        PieceKind::T => 2,
        PieceKind::S => 3,
        PieceKind::Z => 4,
        PieceKind::J => 5,
        PieceKind::L => 6,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PatternPiecePositionIndexError {
    UniverseMismatch,
    PatternIdCapacityExceeded,
    CoverageWordCountMismatch,
    StorageOverflow,
    StorageUnavailable,
}

#[cfg(test)]
mod tests {
    use super::PatternPiecePositionIndex;

    #[test]
    fn global_pattern_lookup_uses_the_dense_sorted_subset_index() {
        let index = PatternPiecePositionIndex {
            global_pattern_count: 12,
            local_pattern_ids: vec![1, 4, 7, 11],
            sequence_len: 0,
            word_count: 1,
            position_piece_words: Vec::new(),
        };

        assert_eq!(index.local_pattern_index(1), Some(0));
        assert_eq!(index.local_pattern_index(7), Some(2));
        assert_eq!(index.local_pattern_index(11), Some(3));
        assert_eq!(index.local_pattern_index(6), None);
        assert_eq!(index.local_pattern_index(usize::MAX), None);
    }
}
