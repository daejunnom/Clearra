use clearra_core_domain::piece::piece_kind::PieceKind;
use clearra_coverage::pattern::pattern_bitset::PatternBitSet;

use super::materialized_pattern_universe::{
    MaterializedPatternUniverse, MaterializedPatternUniverseStructure,
};

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

pub enum PatternPiecePositionIndexCompileAdvance {
    Pending,
    Complete(PatternPiecePositionIndex),
}

enum PatternPiecePositionIndexCompileStage {
    ScanSequenceLength { next_pattern: usize },
    Allocate,
    Populate { next_pattern: usize },
    Finished,
}

/// Bounded compiler for the bit-sliced pattern-position index.
///
/// Large factorized setup universes contain millions of logical patterns.
/// Keeping their expansion behind an explicit cursor lets cooperative hosts
/// yield between bounded batches without changing canonical pattern ordering.
pub struct PatternPiecePositionIndexCompileSession {
    universe: MaterializedPatternUniverse,
    all_patterns: bool,
    local_pattern_ids: Vec<u32>,
    pattern_count: usize,
    sequence_len: usize,
    word_count: usize,
    position_piece_words: Vec<u64>,
    sequence: Vec<PieceKind>,
    stage: PatternPiecePositionIndexCompileStage,
}

impl PatternPiecePositionIndexCompileSession {
    pub fn new(
        universe: MaterializedPatternUniverse,
    ) -> Result<Self, PatternPiecePositionIndexError> {
        let pattern_count = universe.pattern_count();
        if pattern_count != 0 {
            u32::try_from(pattern_count - 1)
                .map_err(|_| PatternPiecePositionIndexError::PatternIdCapacityExceeded)?;
        }
        Self::from_selection(universe, true, Vec::new(), pattern_count)
    }

    pub fn new_for_pattern_ids(
        universe: MaterializedPatternUniverse,
        local_pattern_ids: Vec<u32>,
    ) -> Result<Self, PatternPiecePositionIndexError> {
        if !local_pattern_ids.windows(2).all(|ids| ids[0] < ids[1])
            || local_pattern_ids
                .last()
                .is_some_and(|id| *id as usize >= universe.pattern_count())
        {
            return Err(PatternPiecePositionIndexError::PatternIdSelectionInvalid);
        }
        let pattern_count = local_pattern_ids.len();
        Self::from_selection(universe, false, local_pattern_ids, pattern_count)
    }

    fn from_selection(
        universe: MaterializedPatternUniverse,
        all_patterns: bool,
        local_pattern_ids: Vec<u32>,
        pattern_count: usize,
    ) -> Result<Self, PatternPiecePositionIndexError> {
        let (sequence_len, stage) = if pattern_count == 0 {
            (0, PatternPiecePositionIndexCompileStage::Allocate)
        } else {
            match universe.structure() {
                MaterializedPatternUniverseStructure::Explicit => (
                    0,
                    PatternPiecePositionIndexCompileStage::ScanSequenceLength { next_pattern: 0 },
                ),
                MaterializedPatternUniverseStructure::Standard7BagLexicographic {
                    sequence_len,
                }
                | MaterializedPatternUniverseStructure::FactorizedQueueExpression {
                    sequence_len,
                }
                | MaterializedPatternUniverseStructure::ObservedStandard7BagLexicographic {
                    sequence_len,
                    ..
                } => (
                    usize::from(sequence_len),
                    PatternPiecePositionIndexCompileStage::Allocate,
                ),
            }
        };
        Ok(Self {
            universe,
            all_patterns,
            local_pattern_ids,
            pattern_count,
            sequence_len,
            word_count: pattern_count.div_ceil(u64::BITS as usize),
            position_piece_words: Vec::new(),
            sequence: Vec::new(),
            stage,
        })
    }

    pub fn completed_patterns(&self) -> usize {
        match self.stage {
            PatternPiecePositionIndexCompileStage::ScanSequenceLength { next_pattern }
            | PatternPiecePositionIndexCompileStage::Populate { next_pattern } => next_pattern,
            PatternPiecePositionIndexCompileStage::Allocate => 0,
            PatternPiecePositionIndexCompileStage::Finished => self.pattern_count,
        }
    }

    pub const fn pattern_count(&self) -> usize {
        self.pattern_count
    }

    pub fn advance(
        &mut self,
        work_budget: usize,
    ) -> Result<PatternPiecePositionIndexCompileAdvance, PatternPiecePositionIndexError> {
        let budget = work_budget.max(1);
        let stage = core::mem::replace(
            &mut self.stage,
            PatternPiecePositionIndexCompileStage::Finished,
        );
        match stage {
            PatternPiecePositionIndexCompileStage::ScanSequenceLength { mut next_pattern } => {
                let end = next_pattern.saturating_add(budget).min(self.pattern_count);
                while next_pattern < end {
                    let pattern_id = self.pattern_id_at(next_pattern);
                    self.sequence_len = self
                        .sequence_len
                        .max(self.universe.sequence_len_at(pattern_id));
                    next_pattern += 1;
                }
                self.stage = if next_pattern == self.pattern_count {
                    PatternPiecePositionIndexCompileStage::Allocate
                } else {
                    PatternPiecePositionIndexCompileStage::ScanSequenceLength { next_pattern }
                };
                Ok(PatternPiecePositionIndexCompileAdvance::Pending)
            }
            PatternPiecePositionIndexCompileStage::Allocate => {
                let storage_len = self
                    .sequence_len
                    .checked_mul(STANDARD_PIECE_COUNT)
                    .and_then(|count| count.checked_mul(self.word_count))
                    .ok_or(PatternPiecePositionIndexError::StorageOverflow)?;
                self.position_piece_words
                    .try_reserve_exact(storage_len)
                    .map_err(|_| PatternPiecePositionIndexError::StorageUnavailable)?;
                self.position_piece_words.resize(storage_len, 0);
                if self.all_patterns {
                    self.local_pattern_ids
                        .try_reserve_exact(self.pattern_count)
                        .map_err(|_| PatternPiecePositionIndexError::StorageUnavailable)?;
                }
                self.sequence.reserve(self.sequence_len);
                self.stage = PatternPiecePositionIndexCompileStage::Populate { next_pattern: 0 };
                Ok(PatternPiecePositionIndexCompileAdvance::Pending)
            }
            PatternPiecePositionIndexCompileStage::Populate { mut next_pattern } => {
                let end = next_pattern.saturating_add(budget).min(self.pattern_count);
                while next_pattern < end {
                    let global_pattern_id = self.pattern_id_at(next_pattern);
                    if self.all_patterns {
                        self.local_pattern_ids
                            .push(u32::try_from(global_pattern_id).map_err(|_| {
                                PatternPiecePositionIndexError::PatternIdCapacityExceeded
                            })?);
                    }
                    let word_index = next_pattern / u64::BITS as usize;
                    let bit = 1_u64 << (next_pattern % u64::BITS as usize);
                    self.universe
                        .write_sequence_at(global_pattern_id, &mut self.sequence);
                    for (position, piece) in self.sequence.iter().copied().enumerate() {
                        let piece_index = standard_piece_index(piece);
                        let index = ((position * STANDARD_PIECE_COUNT + piece_index)
                            * self.word_count)
                            + word_index;
                        self.position_piece_words[index] |= bit;
                    }
                    next_pattern += 1;
                }
                if next_pattern != self.pattern_count {
                    self.stage = PatternPiecePositionIndexCompileStage::Populate { next_pattern };
                    return Ok(PatternPiecePositionIndexCompileAdvance::Pending);
                }
                Ok(PatternPiecePositionIndexCompileAdvance::Complete(
                    PatternPiecePositionIndex {
                        global_pattern_count: self.universe.pattern_count(),
                        local_pattern_ids: core::mem::take(&mut self.local_pattern_ids),
                        sequence_len: self.sequence_len,
                        word_count: self.word_count,
                        position_piece_words: core::mem::take(&mut self.position_piece_words),
                    },
                ))
            }
            PatternPiecePositionIndexCompileStage::Finished => {
                Err(PatternPiecePositionIndexError::CompileSessionFinished)
            }
        }
    }

    fn pattern_id_at(&self, local_pattern_index: usize) -> usize {
        if self.all_patterns {
            local_pattern_index
        } else {
            self.local_pattern_ids[local_pattern_index] as usize
        }
    }
}

impl PatternPiecePositionIndex {
    pub fn compile(
        universe: &MaterializedPatternUniverse,
    ) -> Result<Self, PatternPiecePositionIndexError> {
        let mut session = PatternPiecePositionIndexCompileSession::new(universe.clone())?;
        loop {
            match session.advance(usize::MAX)? {
                PatternPiecePositionIndexCompileAdvance::Pending => {}
                PatternPiecePositionIndexCompileAdvance::Complete(index) => return Ok(index),
            }
        }
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
        let mut session = PatternPiecePositionIndexCompileSession::new_for_pattern_ids(
            universe.clone(),
            pattern_ids,
        )?;
        loop {
            match session.advance(usize::MAX)? {
                PatternPiecePositionIndexCompileAdvance::Pending => {}
                PatternPiecePositionIndexCompileAdvance::Complete(index) => return Ok(index),
            }
        }
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
    PatternIdSelectionInvalid,
    CompileSessionFinished,
}

#[cfg(test)]
mod tests {
    use clearra_core_domain::{
        piece::piece_kind::PieceKind, probability::probability_value::ProbabilityValue,
    };
    use clearra_coverage::universe::{
        pattern_universe_id::PatternUniverseId, pattern_weight_model_id::PatternWeightModelId,
    };

    use super::{
        MaterializedPatternUniverse, PatternPiecePositionIndex,
        PatternPiecePositionIndexCompileAdvance, PatternPiecePositionIndexCompileSession,
    };

    fn explicit_universe() -> MaterializedPatternUniverse {
        MaterializedPatternUniverse::from_sequences(
            PatternUniverseId::new(1),
            PatternWeightModelId::new(2),
            vec![
                vec![PieceKind::I, PieceKind::O],
                vec![PieceKind::O, PieceKind::I],
                vec![PieceKind::I, PieceKind::I],
            ],
            vec![
                ProbabilityValue::new(0.25).expect("weight"),
                ProbabilityValue::new(0.25).expect("weight"),
                ProbabilityValue::new(0.5).expect("weight"),
            ],
            3,
            true,
            None,
        )
        .expect("explicit universe")
    }

    #[test]
    fn bounded_compiler_preserves_explicit_pattern_order_and_yields() {
        let mut session =
            PatternPiecePositionIndexCompileSession::new(explicit_universe()).expect("compiler");
        let mut pending = 0;
        let index = loop {
            match session.advance(1).expect("compile step") {
                PatternPiecePositionIndexCompileAdvance::Pending => pending += 1,
                PatternPiecePositionIndexCompileAdvance::Complete(index) => break index,
            }
        };

        assert!(pending > 1);
        assert_eq!(index.global_pattern_count(), 3);
        assert_eq!(index.local_pattern_count(), 3);
        assert_eq!(index.sequence_len(), 2);
        assert_eq!(index.word_count(), 1);
        assert_eq!(index.piece_word(0, 1, 0), 0b101);
        assert_eq!(index.piece_word(0, 2, 0), 0b010);
        assert_eq!(index.piece_word(1, 1, 0), 0b110);
        assert_eq!(index.piece_word(1, 2, 0), 0b001);
    }

    #[test]
    fn empty_selected_subset_keeps_the_legacy_zero_length_shape() {
        let mut session = PatternPiecePositionIndexCompileSession::new_for_pattern_ids(
            explicit_universe(),
            Vec::new(),
        )
        .expect("empty subset compiler");
        let index = loop {
            match session.advance(1).expect("compile step") {
                PatternPiecePositionIndexCompileAdvance::Pending => {}
                PatternPiecePositionIndexCompileAdvance::Complete(index) => break index,
            }
        };

        assert_eq!(index.global_pattern_count(), 3);
        assert_eq!(index.local_pattern_count(), 0);
        assert_eq!(index.sequence_len(), 0);
        assert_eq!(index.word_count(), 0);
    }

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
