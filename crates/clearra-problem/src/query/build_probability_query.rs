use clearra_core_domain::board::standard_pc_board::{
    Board256Mask, BOARD256_WORD_COUNT, STANDARD_PC_BOARD_WIDTH, STANDARD_PC_COMPACT_MAX_LINES,
    STANDARD_PC_MAX_LINES,
};
use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_core_domain::solution::StandardBoard64ColoredTilingIdentity;
use clearra_objectives::policy::score_objective_policy::{
    ScoreProfileSelection, SpinProfileSelection,
};
use clearra_pc_graph::request::{PcScenarioQuery, PcSolutionProbabilityPolicy};
use clearra_supply::QueueObservationPolicy;

/// Whether a Build query must materialize exact probability evidence for every
/// canonical solution in its final solution set.
///
/// This policy is intentionally distinct from the PC-family policy. The
/// embedded `PcScenarioQuery` carries the corresponding value only as an
/// executor transport detail; callers select Build semantics through this
/// type.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum BuildSolutionProbabilityPolicy {
    #[default]
    Omit,
    Include,
}

impl BuildSolutionProbabilityPolicy {
    pub const fn requested(self) -> bool {
        matches!(self, Self::Include)
    }
    const fn executor_transport(self) -> PcSolutionProbabilityPolicy {
        match self {
            Self::Omit => PcSolutionProbabilityPolicy::Omit,
            Self::Include => PcSolutionProbabilityPolicy::Include,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum FinesseMetric {
    #[default]
    Off,
    Inputs,
}

impl FinesseMetric {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Inputs => "inputs",
        }
    }

    pub const fn requested(self) -> bool {
        matches!(self, Self::Inputs)
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "off" | "none" => Some(Self::Off),
            "inputs" | "minimum-inputs" => Some(Self::Inputs),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum FinessePatternKnowledge {
    #[default]
    Both,
    Oracle,
    VisibleSeven,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FinessePlacement {
    piece: PieceKind,
    rotation: RotationState,
    x: i16,
    y: i16,
}

impl FinessePlacement {
    pub const fn new(piece: PieceKind, rotation: RotationState, x: i16, y: i16) -> Self {
        Self {
            piece,
            rotation,
            x,
            y,
        }
    }

    pub const fn piece(self) -> PieceKind {
        self.piece
    }
    pub const fn rotation(self) -> RotationState {
        self.rotation
    }
    pub const fn x(self) -> i16 {
        self.x
    }
    pub const fn y(self) -> i16 {
        self.y
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinesseScoreRequest {
    placements: Vec<FinessePlacement>,
    initial_cleared_rows: u32,
}

impl FinesseScoreRequest {
    pub const MAX_PLACEMENTS: usize = 60;

    pub fn new(placements: Vec<FinessePlacement>) -> Option<Self> {
        (1..=Self::MAX_PLACEMENTS)
            .contains(&placements.len())
            .then_some(Self {
                placements,
                initial_cleared_rows: 0,
            })
    }

    pub fn placements(&self) -> &[FinessePlacement] {
        &self.placements
    }

    pub const fn initial_cleared_rows(&self) -> u32 {
        self.initial_cleared_rows
    }

    const fn with_initial_cleared_rows(mut self, rows: u32) -> Self {
        self.initial_cleared_rows = rows;
        self
    }

    /// Returns heap bytes retained by the placement vector using its actual
    /// allocation capacity. The inline request owner is excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        checked_capacity_bytes(
            self.placements.capacity() as u128,
            core::mem::size_of::<FinessePlacement>() as u128,
        )
    }
}

/// Mutually exclusive finesse request carried by a build-probability query.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum BuildProbabilityFinesseRequest {
    #[default]
    Off,
    Search {
        pattern_knowledge: FinessePatternKnowledge,
    },
    Score {
        pattern_knowledge: FinessePatternKnowledge,
        request: FinesseScoreRequest,
    },
}

impl BuildProbabilityFinesseRequest {
    pub const fn metric(&self) -> FinesseMetric {
        match self {
            Self::Off => FinesseMetric::Off,
            Self::Search { .. } | Self::Score { .. } => FinesseMetric::Inputs,
        }
    }

    pub const fn pattern_knowledge(&self) -> FinessePatternKnowledge {
        match self {
            Self::Off => FinessePatternKnowledge::Both,
            Self::Search { pattern_knowledge }
            | Self::Score {
                pattern_knowledge, ..
            } => *pattern_knowledge,
        }
    }

    pub const fn score(&self) -> Option<&FinesseScoreRequest> {
        match self {
            Self::Score { request, .. } => Some(request),
            Self::Off | Self::Search { .. } => None,
        }
    }

    /// Returns the heap payload owned by the active finesse request variant.
    /// Off and Search are inline; Score owns exactly one placement vector.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        match self {
            Self::Off | Self::Search { .. } => Some(0),
            Self::Score { request, .. } => request.checked_retained_capacity_bytes(),
        }
    }
}

fn checked_capacity_bytes(capacity: u128, item_size: u128) -> Option<u128> {
    capacity.checked_mul(item_size)
}

impl FinessePatternKnowledge {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Both => "both",
            Self::Oracle => "oracle",
            Self::VisibleSeven => "visible-7",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "both" => Some(Self::Both),
            "oracle" | "full" | "full-future" => Some(Self::Oracle),
            "visible-7" | "online" | "seven-visible" => Some(Self::VisibleSeven),
            _ => None,
        }
    }
}

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
        Self::from_words_with_height_policy(height, base_words, target_words, false)
    }

    /// Construct a field while retaining the caller's visible height.
    ///
    /// Build search normally contracts empty rows because they do not affect
    /// inverse geometry. Finesse scoring is different: the spawn pose and
    /// movement state space depend on the declared height even when the upper
    /// rows are empty.
    pub fn from_words_preserving_height(
        height: u8,
        base_words: [u64; BOARD256_WORD_COUNT],
        target_words: [u64; BOARD256_WORD_COUNT],
    ) -> Result<Self, BuildProbabilityFieldError> {
        Self::from_words_with_height_policy(height, base_words, target_words, true)
    }

    fn from_words_with_height_policy(
        height: u8,
        base_words: [u64; BOARD256_WORD_COUNT],
        target_words: [u64; BOARD256_WORD_COUNT],
        preserve_height: bool,
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
            height: if preserve_height {
                height
            } else {
                occupied_height(base.union(target))
            },
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

    /// Applies the line clear that precedes a finesse search without changing
    /// the declared spawn height.
    ///
    /// Only rows completed by the submitted base field are cleared. Target
    /// cells are moved by the same row mapping, so a target above a completed
    /// input row keeps its physical relationship to the remaining base. This
    /// operation is intentionally opt-in through the finesse query builders;
    /// ordinary build-probability geometry keeps its existing input contract.
    pub fn after_initial_line_clear(self) -> Self {
        // Preserve the pre-existing fail-closed overlap contract. Dropping a
        // target cell together with a completed base row here would otherwise
        // hide invalid input from the app-level validator.
        if self.base.intersects(self.target) {
            return self;
        }
        let mut compacted_base = [0_u64; BOARD256_WORD_COUNT];
        let mut compacted_target = [0_u64; BOARD256_WORD_COUNT];
        let mut destination_row = 0_u16;
        let mut cleared_any = false;
        let height = u16::from(self.height);

        for source_row in 0..height {
            let source_start = source_row * STANDARD_PC_BOARD_WIDTH;
            let base_row_is_full =
                (0..STANDARD_PC_BOARD_WIDTH).all(|x| self.base.contains_index(source_start + x));
            if base_row_is_full {
                cleared_any = true;
                continue;
            }

            let destination_start = destination_row * STANDARD_PC_BOARD_WIDTH;
            for x in 0..STANDARD_PC_BOARD_WIDTH {
                let source = source_start + x;
                let destination = destination_start + x;
                if self.base.contains_index(source) {
                    set_word_cell(&mut compacted_base, destination);
                }
                if self.target.contains_index(source) {
                    set_word_cell(&mut compacted_target, destination);
                }
            }
            destination_row += 1;
        }

        if !cleared_any {
            return self;
        }
        Self {
            height: self.height,
            base: Board256Mask::from_words(compacted_base),
            target: Board256Mask::from_words(compacted_target),
            include_horizontal_mirror: self.include_horizontal_mirror,
        }
    }

    pub fn completed_base_row_mask(self) -> u32 {
        let mut rows = 0_u32;
        for row in 0..u16::from(self.height) {
            let start = row * STANDARD_PC_BOARD_WIDTH;
            if (0..STANDARD_PC_BOARD_WIDTH).all(|x| self.base.contains_index(start + x)) {
                rows |= 1_u32 << row;
            }
        }
        rows
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

fn set_word_cell(words: &mut [u64; BOARD256_WORD_COUNT], cell: u16) {
    let cell = usize::from(cell);
    words[cell / u64::BITS as usize] |= 1_u64 << (cell % u64::BITS as usize);
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
mod field_tests {
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

    #[test]
    fn finesse_initial_clear_moves_base_and_target_together_and_preserves_height() {
        let mut base = [0_u64; 4];
        base[0] = 0x3ff | (1_u64 << 10) | (1_u64 << 39);
        let target = [(2_u16, 2_u16), (5, 3)]
            .into_iter()
            .fold([0_u64; 4], |mut words, (x, y)| {
                super::set_word_cell(&mut words, y * 10 + x);
                words
            });
        let field = BuildProbabilityField::from_words_preserving_height(4, base, target)
            .unwrap()
            .with_horizontal_mirror_included(true)
            .after_initial_line_clear();

        assert_eq!(field.height(), 4);
        assert!(field.includes_horizontal_mirror());
        assert_eq!(field.base_words(), [1 | (1_u64 << 29), 0, 0, 0]);
        assert_eq!(
            field.target_words(),
            [(1_u64 << 12) | (1_u64 << 25), 0, 0, 0]
        );
    }

    #[test]
    fn finesse_initial_clear_handles_multiple_extended_rows_across_word_boundaries() {
        let mut base = [0_u64; 4];
        for x in 0..10 {
            super::set_word_cell(&mut base, 10 + x);
            super::set_word_cell(&mut base, 60 + x);
        }
        for (x, y) in [(7_u16, 0_u16), (3, 2), (9, 7)] {
            super::set_word_cell(&mut base, y * 10 + x);
        }
        let mut target = [0_u64; 4];
        for (x, y) in [(2_u16, 3_u16), (4, 5), (1, 7)] {
            super::set_word_cell(&mut target, y * 10 + x);
        }

        let field = BuildProbabilityField::from_words_preserving_height(8, base, target)
            .unwrap()
            .after_initial_line_clear();
        let mut expected_base = [0_u64; 4];
        for (x, y) in [(7_u16, 0_u16), (3, 1), (9, 5)] {
            super::set_word_cell(&mut expected_base, y * 10 + x);
        }
        let mut expected_target = [0_u64; 4];
        for (x, y) in [(2_u16, 2_u16), (4, 4), (1, 5)] {
            super::set_word_cell(&mut expected_target, y * 10 + x);
        }

        assert_eq!(field.height(), 8);
        assert_eq!(field.base_words(), expected_base);
        assert_eq!(field.target_words(), expected_target);
    }

    #[test]
    fn finesse_initial_clear_does_not_hide_base_target_overlap() {
        let field =
            BuildProbabilityField::from_words_preserving_height(4, [0x3ff, 0, 0, 0], [1, 0, 0, 0])
                .unwrap();

        let unchanged = field.after_initial_line_clear();
        assert_eq!(unchanged, field);
        assert!(unchanged.base().intersects(unchanged.target()));
    }
}

/// A fixed-field buildability query whose field remains authoritative outside
/// the compact PC board carried by the supply/rule query.
#[derive(Clone, Debug, PartialEq)]
pub struct BuildProbabilityQuery {
    core_query: PcScenarioQuery,
    input_field: BuildProbabilityField,
    field: BuildProbabilityField,
    aggregation: BuildProbabilityAggregation,
    finesse: BuildProbabilityFinesseRequest,
    solution_probability_policy: BuildSolutionProbabilityPolicy,
}

impl BuildProbabilityQuery {
    pub fn new(mut core_query: PcScenarioQuery, field: BuildProbabilityField) -> Self {
        // A PC-family option must not silently acquire Build-family authority.
        // The Build policy below is the sole public owner and synchronizes this
        // executor transport whenever it changes.
        core_query = core_query.with_solution_probability_policy(PcSolutionProbabilityPolicy::Omit);
        Self {
            core_query,
            input_field: field,
            field,
            aggregation: BuildProbabilityAggregation::Buildability,
            finesse: BuildProbabilityFinesseRequest::Off,
            solution_probability_policy: BuildSolutionProbabilityPolicy::Omit,
        }
    }

    pub const fn with_aggregation(mut self, aggregation: BuildProbabilityAggregation) -> Self {
        self.aggregation = aggregation;
        self
    }

    pub fn with_solution_probability_policy(
        mut self,
        policy: BuildSolutionProbabilityPolicy,
    ) -> Self {
        self.core_query = self
            .core_query
            .with_solution_probability_policy(policy.executor_transport());
        self.solution_probability_policy = policy;
        self
    }

    /// Selects the Build queue-information contract and synchronizes the
    /// embedded scenario transport. `VisibleSeven` is evaluated by the shared
    /// HoldAutomaton observation-class policy; it is not a display-only hint.
    pub fn with_queue_observation_policy(mut self, policy: QueueObservationPolicy) -> Self {
        self.core_query = self.core_query.with_queue_observation_policy(policy);
        self
    }

    /// Requests the shared replay-backed score matrix for a Build product.
    ///
    /// This is deliberately a Build-owned adapter instead of allowing an App
    /// caller to mutate the embedded PC transport directly.  The objective
    /// kind (all/unique/minimum-cover) remains unchanged; score is retained as
    /// evidence for the Build reducer, and attack never becomes an ordering or
    /// equality coordinate.
    pub fn with_score_summary(mut self, profile: ScoreProfileSelection, initial_b2b: u16) -> Self {
        let objective = self
            .core_query
            .objective()
            .with_score_summary()
            .with_score_profile(profile)
            .with_initial_b2b(u32::from(initial_b2b));
        self.core_query = self.core_query.with_objective(objective);
        self
    }

    /// Restricts replay evidence to paths that preserve back-to-back under the
    /// selected spin contract.  This powers `build.evaluate.b2b-cover`; it is
    /// not a display-only post-filter and therefore enters the compiled search
    /// problem before candidate execution begins.
    pub fn with_back_to_back_preservation(mut self, spin_profile: SpinProfileSelection) -> Self {
        let objective = self
            .core_query
            .objective()
            .with_back_to_back_preservation(spin_profile);
        self.core_query = self.core_query.with_objective(objective);
        self
    }

    /// Restricts actual Build geometry/replay evaluation to a normalized
    /// supplied colored-field set. The embedded scenario query owns the sorted,
    /// deduplicated identities; this method is not a display-only filter.
    pub fn with_allowed_colored_solution_identities(
        mut self,
        identities: impl IntoIterator<Item = StandardBoard64ColoredTilingIdentity>,
    ) -> Self {
        self.core_query = self
            .core_query
            .with_allowed_colored_solution_identities(identities);
        self
    }

    pub fn with_finesse(
        mut self,
        metric: FinesseMetric,
        pattern_knowledge: FinessePatternKnowledge,
    ) -> Self {
        self.finesse = match metric {
            FinesseMetric::Off => {
                self.field = self.input_field;
                BuildProbabilityFinesseRequest::Off
            }
            FinesseMetric::Inputs => {
                self.field = self.input_field.after_initial_line_clear();
                BuildProbabilityFinesseRequest::Search { pattern_knowledge }
            }
        };
        self
    }

    pub fn with_finesse_score(mut self, score: FinesseScoreRequest) -> Self {
        let pattern_knowledge = self.finesse.pattern_knowledge();
        let initial_cleared_rows = self.input_field.completed_base_row_mask();
        self.field = self.input_field.after_initial_line_clear();
        self.finesse = BuildProbabilityFinesseRequest::Score {
            pattern_knowledge,
            request: score.with_initial_cleared_rows(initial_cleared_rows),
        };
        self
    }

    pub fn core_query(&self) -> &PcScenarioQuery {
        &self.core_query
    }

    pub const fn queue_observation_policy(&self) -> QueueObservationPolicy {
        self.core_query.queue_observation_policy()
    }

    pub fn allowed_colored_solution_identities(
        &self,
    ) -> Option<&[StandardBoard64ColoredTilingIdentity]> {
        self.core_query.allowed_colored_solution_identities()
    }

    /// Splits the typed Build owner for the finite compiler without cloning
    /// either the scenario queue or the active finesse request. The original
    /// input field is no longer needed after validation; the normalized search
    /// field is returned with the response policies that outlive compilation.
    pub fn into_finite_compile_parts(
        self,
    ) -> (
        PcScenarioQuery,
        BuildProbabilityField,
        BuildProbabilityAggregation,
        BuildProbabilityFinesseRequest,
        BuildSolutionProbabilityPolicy,
    ) {
        let Self {
            core_query,
            input_field: _,
            field,
            aggregation,
            finesse,
            solution_probability_policy,
        } = self;
        (
            core_query,
            field,
            aggregation,
            finesse,
            solution_probability_policy,
        )
    }

    pub const fn field(&self) -> BuildProbabilityField {
        self.field
    }

    pub const fn aggregation(&self) -> BuildProbabilityAggregation {
        self.aggregation
    }

    pub const fn solution_probability_policy(&self) -> BuildSolutionProbabilityPolicy {
        self.solution_probability_policy
    }

    pub const fn finesse_metric(&self) -> FinesseMetric {
        self.finesse.metric()
    }

    pub const fn finesse_pattern_knowledge(&self) -> FinessePatternKnowledge {
        self.finesse.pattern_knowledge()
    }

    pub fn finesse_score(&self) -> Option<&FinesseScoreRequest> {
        self.finesse.score()
    }

    pub const fn finesse_request(&self) -> &BuildProbabilityFinesseRequest {
        &self.finesse
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

    /// Returns the complete query-owned heap graph fieldwise using actual
    /// allocation capacities. The embedded scenario query and active finesse
    /// request are the only heap owners; build fields and policies are inline.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        self.core_query
            .checked_build_probability_retained_capacity_bytes()?
            .checked_add(self.finesse.checked_retained_capacity_bytes()?)
    }
}

#[cfg(test)]
mod query_tests {
    use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
    use clearra_objectives::policy::score_objective_policy::{
        ScoreProfileSelection, SpinProfileSelection,
    };
    use clearra_pc_graph::request::{PcQueueInput, PcScenarioBoard, PcScenarioQuery, PieceWindow};
    use clearra_supply::queue::fixed_sequence::FixedSequence;

    use super::{
        BuildProbabilityField, BuildProbabilityFinesseRequest, BuildProbabilityQuery,
        BuildSolutionProbabilityPolicy, FinesseMetric, FinessePatternKnowledge, FinessePlacement,
        FinesseScoreRequest,
    };

    #[test]
    fn ordinary_build_field_contracts_unused_upper_rows() {
        let field = BuildProbabilityField::from_words(8, [1, 0, 0, 0], [0; 4]).unwrap();
        assert_eq!(field.height(), 1);
    }

    #[test]
    fn finesse_score_field_preserves_spawn_height() {
        let field =
            BuildProbabilityField::from_words_preserving_height(8, [1, 0, 0, 0], [0; 4]).unwrap();
        assert_eq!(field.height(), 8);
    }

    #[test]
    fn finesse_query_opt_in_matches_core_initial_line_clear_without_affecting_off_mode() {
        let base = 0x3ff | (1_u64 << 10);
        let target = (1_u64 << 24) | (1_u64 << 25);
        let field = BuildProbabilityField::from_words_preserving_height(
            4,
            [base, 0, 0, 0],
            [target, 0, 0, 0],
        )
        .unwrap();
        let core = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, base),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));

        let off = BuildProbabilityQuery::new(core.clone(), field)
            .with_finesse(FinesseMetric::Off, FinessePatternKnowledge::Both);
        assert_eq!(off.field(), field);

        let inputs = BuildProbabilityQuery::new(core.clone(), field)
            .with_finesse(FinesseMetric::Inputs, FinessePatternKnowledge::Oracle);
        assert_eq!(inputs.field().height(), 4);
        assert_eq!(inputs.field().compact_base_mask(), Some(1));
        assert_eq!(
            inputs.field().compact_target_mask(),
            Some((1_u64 << 14) | (1_u64 << 15))
        );
        assert_eq!(
            inputs.field().compact_base_mask(),
            Some(
                core.initial_board()
                    .after_initial_line_clear()
                    .occupied_mask()
            )
        );
    }

    #[test]
    fn finesse_request_is_tagged_and_cannot_retain_score_while_off() {
        let core = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let field =
            BuildProbabilityField::from_words_preserving_height(4, [0x3ff, 0, 0, 0], [0; 4])
                .unwrap();
        let score = FinesseScoreRequest::new(vec![FinessePlacement::new(
            PieceKind::O,
            RotationState::Zero,
            4,
            1,
        )])
        .unwrap();
        let query = BuildProbabilityQuery::new(core, field)
            .with_finesse(FinesseMetric::Inputs, FinessePatternKnowledge::VisibleSeven)
            .with_finesse_score(score);
        assert!(matches!(
            query.finesse_request(),
            BuildProbabilityFinesseRequest::Score {
                pattern_knowledge: FinessePatternKnowledge::VisibleSeven,
                ..
            }
        ));

        let off = query.with_finesse(FinesseMetric::Off, FinessePatternKnowledge::Oracle);
        assert!(matches!(
            off.finesse_request(),
            BuildProbabilityFinesseRequest::Off
        ));
        assert_eq!(off.field(), field);
        assert_eq!(off.finesse_metric(), FinesseMetric::Off);
        assert!(off.finesse_score().is_none());
        assert_eq!(
            off.checked_retained_capacity_bytes(),
            off.core_query()
                .checked_build_probability_retained_capacity_bytes()
        );
    }

    #[test]
    fn finesse_score_request_enforces_the_ctk3_page_limit() {
        let placement = FinessePlacement::new(PieceKind::O, RotationState::Zero, 4, 0);
        assert!(FinesseScoreRequest::new(vec![placement; 60]).is_some());
        assert!(FinesseScoreRequest::new(vec![placement; 61]).is_none());
    }

    #[test]
    fn retained_capacity_counts_scenario_and_active_finesse_buffers_once() {
        let mut pieces = Vec::with_capacity(31);
        pieces.push(PieceKind::O);
        let core = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(pieces)),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let field =
            BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0x0c03, 0, 0, 0])
                .expect("one-piece field");
        let mut placements = Vec::with_capacity(17);
        placements.push(FinessePlacement::new(
            PieceKind::O,
            RotationState::Zero,
            4,
            0,
        ));
        let placement_capacity = placements.capacity();
        let score = FinesseScoreRequest::new(placements).expect("one placement is valid");
        let query = BuildProbabilityQuery::new(core, field).with_finesse_score(score);
        let expected = query
            .core_query()
            .checked_build_probability_retained_capacity_bytes()
            .and_then(|bytes| {
                bytes.checked_add(
                    (placement_capacity as u128)
                        .checked_mul(core::mem::size_of::<FinessePlacement>() as u128)?,
                )
            });
        let actual = query
            .checked_retained_capacity_bytes()
            .expect("query capacity fits u128");
        let admitted = |limit| actual <= limit;

        assert_eq!(Some(actual), expected);
        assert!(actual > 0);
        assert!(admitted(actual));
        assert!(!admitted(actual - 1));
    }

    #[test]
    fn retained_capacity_arithmetic_fails_closed_on_overflow() {
        assert_eq!(super::checked_capacity_bytes(u128::MAX, 1), Some(u128::MAX));
        assert_eq!(super::checked_capacity_bytes(u128::MAX, 2), None);
    }

    #[test]
    fn build_solution_probability_policy_is_typed_and_owns_the_executor_transport() {
        use clearra_pc_graph::request::PcSolutionProbabilityPolicy;

        let core = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::O])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1))
        .with_solution_probability_policy(PcSolutionProbabilityPolicy::Include);
        let field =
            BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0x0c03, 0, 0, 0])
                .unwrap();

        let omitted = BuildProbabilityQuery::new(core, field);
        assert_eq!(
            omitted.solution_probability_policy(),
            BuildSolutionProbabilityPolicy::Omit
        );
        assert_eq!(
            omitted.core_query().solution_probability_policy(),
            PcSolutionProbabilityPolicy::Omit
        );

        let included =
            omitted.with_solution_probability_policy(BuildSolutionProbabilityPolicy::Include);
        assert!(included.solution_probability_policy().requested());
        assert_eq!(
            included.core_query().solution_probability_policy(),
            PcSolutionProbabilityPolicy::Include
        );

        let omitted =
            included.with_solution_probability_policy(BuildSolutionProbabilityPolicy::Omit);
        assert!(!omitted.solution_probability_policy().requested());
        assert_eq!(
            omitted.core_query().solution_probability_policy(),
            PcSolutionProbabilityPolicy::Omit
        );
    }

    #[test]
    fn build_score_and_b2b_requests_enter_the_compiled_objective_transport() {
        let core = PcScenarioQuery::new(
            PcScenarioBoard::standard_10(4, 0),
            PcQueueInput::fixed_sequence(FixedSequence::new(vec![PieceKind::I])),
            PieceWindow::new(1),
        )
        .with_exact_pieces(Some(1));
        let field = BuildProbabilityField::from_words_preserving_height(4, [0; 4], [0xf, 0, 0, 0])
            .expect("one-piece Build field");

        let scored = BuildProbabilityQuery::new(core.clone(), field)
            .with_score_summary(ScoreProfileSelection::Guideline, u16::MAX);
        let score = scored.core_query().objective().score();
        assert!(score.requested());
        assert_eq!(score.profile(), ScoreProfileSelection::Guideline);
        assert_eq!(score.initial_b2b(), u32::from(u16::MAX));

        let b2b = BuildProbabilityQuery::new(core, field)
            .with_back_to_back_preservation(SpinProfileSelection::AllSpinPlus);
        let constraints = b2b.core_query().objective().execution_constraints();
        assert!(constraints.preserves_back_to_back());
        assert_eq!(
            constraints.spin_profile(),
            SpinProfileSelection::AllSpinPlus
        );
    }
}
