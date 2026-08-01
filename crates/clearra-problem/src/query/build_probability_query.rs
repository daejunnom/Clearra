use clearra_core_domain::board::standard_pc_board::{
    Board256Mask, BOARD256_WORD_COUNT, STANDARD_PC_BOARD_WIDTH, STANDARD_PC_COMPACT_MAX_LINES,
    STANDARD_PC_MAX_LINES,
};
use clearra_objectives::policy::score_objective_policy::SpinProfileSelection;
use clearra_pc_graph::request::PcScenarioQuery;

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum BuildProbabilityAggregation {
    #[default]
    Buildability,
    TilingOnly,
    SpinSearch {
        profile: SpinProfileSelection,
    },
}

impl BuildProbabilityAggregation {
    pub const fn spin_search(profile: SpinProfileSelection) -> Self {
        Self::SpinSearch { profile }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Buildability => "buildability",
            Self::TilingOnly => "tiling",
            Self::SpinSearch { .. } => "spin",
        }
    }

    pub const fn is_tiling_only(self) -> bool {
        matches!(self, Self::TilingOnly)
    }

    pub const fn requests_spin_coverage(self) -> bool {
        matches!(self, Self::SpinSearch { .. })
    }

    pub const fn spin_coverage_target_id(self) -> Option<&'static str> {
        match self {
            Self::Buildability | Self::TilingOnly => None,
            Self::SpinSearch { profile } => Some(profile.as_str()),
        }
    }

    pub const fn spin_profile(self) -> Option<SpinProfileSelection> {
        match self {
            Self::Buildability | Self::TilingOnly => None,
            Self::SpinSearch { profile } => Some(profile),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BuildProbabilityField {
    height: u8,
    base: Board256Mask,
    target: Board256Mask,
    include_horizontal_mirror: bool,
}

impl BuildProbabilityField {
    pub fn from_words(
        height: u8,
        base_words: [u64; BOARD256_WORD_COUNT],
        target_words: [u64; BOARD256_WORD_COUNT],
    ) -> Result<Self, BuildProbabilityFieldError> {
        if height == 0 || height > STANDARD_PC_MAX_LINES {
            return Err(BuildProbabilityFieldError::HeightOutOfRange { height });
        }
        let cell_count = u16::from(height) * STANDARD_PC_BOARD_WIDTH;
        let base = Board256Mask::from_words(base_words);
        let target = Board256Mask::from_words(target_words);
        if !base.fits_cell_count(cell_count).unwrap_or(false) {
            return Err(BuildProbabilityFieldError::BaseOutsideField { height });
        }
        if !target.fits_cell_count(cell_count).unwrap_or(false) {
            return Err(BuildProbabilityFieldError::TargetOutsideField { height });
        }
        Ok(Self {
            height: occupied_height(base.union(target)),
            base,
            target,
            include_horizontal_mirror: false,
        })
    }

    pub const fn with_horizontal_mirror_included(mut self, included: bool) -> Self {
        self.include_horizontal_mirror = included;
        self
    }

    pub const fn includes_horizontal_mirror(self) -> bool {
        self.include_horizontal_mirror
    }

    pub fn base_is_horizontally_symmetric(self) -> bool {
        let mirrored = self
            .base
            .mirrored_horizontally(STANDARD_PC_BOARD_WIDTH, u16::from(self.height))
            .expect("validated build field remains valid after mirroring");
        mirrored == self.base
    }

    pub fn includes_applicable_horizontal_mirror(self) -> bool {
        self.include_horizontal_mirror && self.base_is_horizontally_symmetric()
    }

    pub const fn original_only(mut self) -> Self {
        self.include_horizontal_mirror = false;
        self
    }

    pub fn mirrored_horizontally(self) -> Self {
        let width = STANDARD_PC_BOARD_WIDTH;
        let height = u16::from(self.height);
        Self {
            height: self.height,
            base: self
                .base
                .mirrored_horizontally(width, height)
                .expect("validated build field remains valid after mirroring"),
            target: self
                .target
                .mirrored_horizontally(width, height)
                .expect("validated build target remains valid after mirroring"),
            include_horizontal_mirror: false,
        }
    }

    pub const fn height(self) -> u8 {
        self.height
    }

    pub const fn width(self) -> u8 {
        STANDARD_PC_BOARD_WIDTH as u8
    }

    pub const fn base(self) -> Board256Mask {
        self.base
    }

    pub const fn target(self) -> Board256Mask {
        self.target
    }

    pub const fn base_words(self) -> [u64; BOARD256_WORD_COUNT] {
        self.base.words()
    }

    pub const fn target_words(self) -> [u64; BOARD256_WORD_COUNT] {
        self.target.words()
    }

    pub const fn target_board(self) -> Board256Mask {
        self.base.union(self.target)
    }

    pub const fn target_piece_count(self) -> usize {
        self.target.count_ones() as usize / 4
    }

    pub const fn is_compact(self) -> bool {
        self.height <= STANDARD_PC_COMPACT_MAX_LINES
    }

    pub const fn compact_base_mask(self) -> Option<u64> {
        if self.is_compact() {
            Some(self.base.words()[0])
        } else {
            None
        }
    }

    pub const fn compact_target_mask(self) -> Option<u64> {
        if self.is_compact() {
            Some(self.target.words()[0])
        } else {
            None
        }
    }

    pub fn compact_final_board_mask(self) -> Option<u64> {
        let board = self.compact_base_mask()? | self.compact_target_mask()?;
        let full_row = (1_u64 << STANDARD_PC_BOARD_WIDTH) - 1;
        let row_width = u32::from(STANDARD_PC_BOARD_WIDTH);
        let mut compacted = 0_u64;
        let mut destination_row = 0_u32;
        for source_row in 0..u32::from(self.height) {
            let row = (board >> (source_row * row_width)) & full_row;
            if row == full_row {
                continue;
            }
            compacted |= row << (destination_row * row_width);
            destination_row += 1;
        }
        Some(compacted)
    }
}

fn occupied_height(mask: Board256Mask) -> u8 {
    let words = mask.words();
    for (word_index, word) in words.into_iter().enumerate().rev() {
        if word == 0 {
            continue;
        }
        let highest_bit = (u64::BITS - 1 - word.leading_zeros()) as usize;
        let highest_cell = word_index * u64::BITS as usize + highest_bit;
        return (highest_cell / usize::from(STANDARD_PC_BOARD_WIDTH) + 1) as u8;
    }
    1
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuildProbabilityFieldError {
    HeightOutOfRange { height: u8 },
    BaseOutsideField { height: u8 },
    TargetOutsideField { height: u8 },
}

#[cfg(test)]
mod tests {
    use super::BuildProbabilityField;

    #[test]
    fn compact_final_board_mask_applies_completed_line_clears() {
        let field = BuildProbabilityField::from_words(3, [0; 4], [0x0fff_ffff, 0, 0, 0]).unwrap();

        assert_eq!(field.compact_final_board_mask(), Some(0xff));
    }

    #[test]
    fn compact_final_board_mask_clears_a_completed_pc_field() {
        let field =
            BuildProbabilityField::from_words(4, [0; 4], [0xff_ffff_ffff, 0, 0, 0]).unwrap();

        assert_eq!(field.compact_final_board_mask(), Some(0));
    }
}

/// A fixed-field buildability query whose field remains authoritative outside
/// the compact PC board carried by the supply/rule query.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildProbabilityQuery {
    core_query: PcScenarioQuery,
    field: BuildProbabilityField,
    aggregation: BuildProbabilityAggregation,
}

impl BuildProbabilityQuery {
    pub fn new(core_query: PcScenarioQuery, field: BuildProbabilityField) -> Self {
        Self {
            core_query,
            field,
            aggregation: BuildProbabilityAggregation::Buildability,
        }
    }

    pub const fn with_aggregation(mut self, aggregation: BuildProbabilityAggregation) -> Self {
        self.aggregation = aggregation;
        self
    }

    pub fn core_query(&self) -> &PcScenarioQuery {
        &self.core_query
    }

    pub const fn field(&self) -> BuildProbabilityField {
        self.field
    }

    pub const fn aggregation(&self) -> BuildProbabilityAggregation {
        self.aggregation
    }

    pub const fn target_cells(&self) -> Option<u64> {
        self.field.compact_target_mask()
    }

    pub const fn initial_board_mask(&self) -> Option<u64> {
        self.field.compact_base_mask()
    }

    pub const fn target_board_mask(&self) -> Option<u64> {
        if self.field.is_compact() {
            Some(self.field.target_board().words()[0])
        } else {
            None
        }
    }

    pub const fn target_piece_count(&self) -> usize {
        self.field.target_piece_count()
    }
}
