use crate::native::CNativeBuildVariantView;

pub const C_COVERAGE_MAX_PATTERNS: usize = 1024;
pub const C_COVERAGE_MAX_WORDS: usize = 16;
pub const C_SCORE_MATRIX_CAPACITY_EXCEEDED: i32 = 7;
pub const C_SPIN_COVERAGE_CAPACITY_EXCEEDED: i32 = 8;
pub const C_COVERAGE_PIECE_SOURCE_MISMATCH: i32 = 9;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CPatternBitSet {
    pub pattern_universe_id: u64,
    pub pattern_weight_model_id: u64,
    pub pattern_count: u32,
    pub word_count: u16,
    pub reserved: u16,
    pub words: [u64; C_COVERAGE_MAX_WORDS],
}

impl CPatternBitSet {
    pub fn word_count_for(pattern_count: u32) -> Option<u16> {
        if pattern_count == 0 || pattern_count as usize > C_COVERAGE_MAX_PATTERNS {
            return None;
        }
        Some(pattern_count.div_ceil(64) as u16)
    }
}
impl CPatternBitSet {
    pub fn empty_with_identity(
        pattern_universe_id: u64,
        pattern_weight_model_id: u64,
        pattern_count: u32,
    ) -> Option<Self> {
        if pattern_universe_id == 0 || pattern_weight_model_id == 0 {
            return None;
        }

        Some(Self {
            pattern_universe_id,
            pattern_weight_model_id,
            pattern_count,
            word_count: Self::word_count_for(pattern_count)?,
            reserved: 0,
            words: [0; C_COVERAGE_MAX_WORDS],
        })
    }
}
impl CPatternBitSet {
    pub fn single_with_identity(
        pattern_universe_id: u64,
        pattern_weight_model_id: u64,
        pattern_count: u32,
        pattern_id: u32,
    ) -> Option<Self> {
        if pattern_id >= pattern_count {
            return None;
        }
        let mut bitset =
            Self::empty_with_identity(pattern_universe_id, pattern_weight_model_id, pattern_count)?;
        bitset.words[(pattern_id / 64) as usize] |= 1_u64 << (pattern_id % 64);
        Some(bitset)
    }
}
impl CPatternBitSet {
    #[cfg(test)]
    pub fn empty(pattern_count: u32) -> Option<Self> {
        Some(Self {
            pattern_universe_id: 0,
            pattern_weight_model_id: 0,
            pattern_count,
            word_count: Self::word_count_for(pattern_count)?,
            reserved: 0,
            words: [0; C_COVERAGE_MAX_WORDS],
        })
    }
}
impl CPatternBitSet {
    #[cfg(test)]
    pub fn single(pattern_count: u32, pattern_id: u32) -> Option<Self> {
        if pattern_id >= pattern_count {
            return None;
        }
        let mut bitset = Self::empty(pattern_count)?;
        bitset.words[(pattern_id / 64) as usize] |= 1_u64 << (pattern_id % 64);
        Some(bitset)
    }
}
impl CPatternBitSet {
    pub fn owned_snapshot(&self) -> Option<OwnedCorePatternBitSetSnapshot> {
        let word_count = self.word_count as usize;
        if word_count > C_COVERAGE_MAX_WORDS {
            return None;
        }
        Some(OwnedCorePatternBitSetSnapshot {
            pattern_universe_id: self.pattern_universe_id,
            pattern_weight_model_id: self.pattern_weight_model_id,
            pattern_count: self.pattern_count,
            words: self.words[..word_count].to_vec(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnedCorePatternBitSetSnapshot {
    pattern_universe_id: u64,
    pattern_weight_model_id: u64,
    pattern_count: u32,
    words: Vec<u64>,
}

impl OwnedCorePatternBitSetSnapshot {
    pub fn pattern_universe_id(&self) -> u64 {
        self.pattern_universe_id
    }
}
impl OwnedCorePatternBitSetSnapshot {
    pub fn pattern_weight_model_id(&self) -> u64 {
        self.pattern_weight_model_id
    }
}
impl OwnedCorePatternBitSetSnapshot {
    pub fn pattern_count(&self) -> u32 {
        self.pattern_count
    }
}
impl OwnedCorePatternBitSetSnapshot {
    pub fn words(&self) -> &[u64] {
        &self.words
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CCoverageRowView {
    pub candidate_id: u64,
    pub piece_source_id: u64,
    pub row_kind: u32,
    pub coverage_pattern_id: u32,
    pub pattern_universe_id: u64,
    pub pattern_weight_model_id: u64,
    pub patterns: CPatternBitSet,
}

impl CCoverageRowView {
    pub fn single_pattern_with_identity(
        candidate_id: u64,
        row_kind: u32,
        pattern_universe_id: u64,
        pattern_weight_model_id: u64,
        pattern_count: u32,
        pattern_id: u32,
    ) -> Option<Self> {
        Self::single_pattern_with_identity_and_piece_source(
            candidate_id,
            0,
            row_kind,
            pattern_universe_id,
            pattern_weight_model_id,
            pattern_count,
            pattern_id,
        )
    }
}
impl CCoverageRowView {
    pub fn single_pattern_with_identity_and_piece_source(
        candidate_id: u64,
        piece_source_id: u64,
        row_kind: u32,
        pattern_universe_id: u64,
        pattern_weight_model_id: u64,
        pattern_count: u32,
        pattern_id: u32,
    ) -> Option<Self> {
        let patterns = CPatternBitSet::single_with_identity(
            pattern_universe_id,
            pattern_weight_model_id,
            pattern_count,
            pattern_id,
        )?;

        Some(Self {
            candidate_id,
            piece_source_id,
            row_kind,
            coverage_pattern_id: pattern_id,
            pattern_universe_id,
            pattern_weight_model_id,
            patterns,
        })
    }
}
impl CCoverageRowView {
    pub fn from_build_variant_with_identity(
        variant: &CNativeBuildVariantView,
        row_kind: u32,
        pattern_universe_id: u64,
        pattern_weight_model_id: u64,
        pattern_count: u32,
    ) -> Option<Self> {
        Self::from_build_variant_with_identity_and_piece_source(
            variant,
            0,
            row_kind,
            pattern_universe_id,
            pattern_weight_model_id,
            pattern_count,
        )
    }
}
impl CCoverageRowView {
    pub fn from_build_variant_with_identity_and_piece_source(
        variant: &CNativeBuildVariantView,
        piece_source_id: u64,
        row_kind: u32,
        pattern_universe_id: u64,
        pattern_weight_model_id: u64,
        pattern_count: u32,
    ) -> Option<Self> {
        Self::single_pattern_with_identity(
            variant.candidate_id,
            row_kind,
            pattern_universe_id,
            pattern_weight_model_id,
            pattern_count,
            variant.coverage_pattern_id,
        )
        .map(|mut row| {
            row.piece_source_id = piece_source_id;
            row
        })
    }
}
impl CCoverageRowView {
    pub fn product_from_build_variant_with_identity(
        variant: &CNativeBuildVariantView,
        piece_source_id: u64,
        row_kind: u32,
        pattern_universe_id: u64,
        pattern_weight_model_id: u64,
        pattern_count: u32,
    ) -> Option<Self> {
        if piece_source_id == 0 {
            return None;
        }
        Self::single_pattern_with_identity_and_piece_source(
            variant.candidate_id,
            piece_source_id,
            row_kind,
            pattern_universe_id,
            pattern_weight_model_id,
            pattern_count,
            variant.coverage_pattern_id,
        )
    }
}
impl CCoverageRowView {
    #[cfg(test)]
    pub fn single_pattern(candidate_id: u64, pattern_count: u32, pattern_id: u32) -> Option<Self> {
        Some(Self {
            candidate_id,
            piece_source_id: 0,
            row_kind: C_COVERAGE_ROW_KIND_BUILD,
            coverage_pattern_id: pattern_id,
            pattern_universe_id: 0,
            pattern_weight_model_id: 0,
            patterns: CPatternBitSet::single(pattern_count, pattern_id)?,
        })
    }
}
impl CCoverageRowView {
    #[cfg(test)]
    pub fn from_build_variant(
        variant: &CNativeBuildVariantView,
        pattern_count: u32,
    ) -> Option<Self> {
        Self::single_pattern(
            variant.candidate_id,
            pattern_count,
            variant.coverage_pattern_id,
        )
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CCoverageOverlapReport {
    pub overlap_count: u32,
    pub has_overlap: u8,
    pub reserved: [u8; 3],
}

pub const C_COVERAGE_ROW_KIND_BUILD: u32 = 2;

#[cfg(test)]
#[path = "coverage_row_view_tests.rs"]
mod tests;
