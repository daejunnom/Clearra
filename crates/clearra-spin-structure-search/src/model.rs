use std::{error::Error, fmt, sync::Arc};

use clearra_core_domain::piece::{piece_kind::PieceKind, rotation::RotationState};
use clearra_replay::ScoringLockEvidence;
use clearra_rules::profile::rule_profile::RuleProfileId;
use clearra_scoring::profile::{SpinProfile, SpinProfileId};

use crate::{board::StructureBoard, operation_catalog::LogicalOperationCatalog};

const PIECE_KIND_COUNT: usize = 7;

/// An unordered, multiplicity-preserving standard-piece source.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PieceInventory {
    counts: [u8; PIECE_KIND_COUNT],
}

impl PieceInventory {
    pub const EMPTY: Self = Self {
        counts: [0; PIECE_KIND_COUNT],
    };

    pub fn from_pieces(
        pieces: impl IntoIterator<Item = PieceKind>,
    ) -> Result<Self, SpinStructureError> {
        let mut inventory = Self::EMPTY;
        for piece in pieces {
            let slot = &mut inventory.counts[piece_index(piece)];
            *slot = slot
                .checked_add(1)
                .ok_or(SpinStructureError::PieceMultiplicityOverflow(piece))?;
        }
        Ok(inventory)
    }

    pub const fn from_counts(counts: [u8; PIECE_KIND_COUNT]) -> Self {
        Self { counts }
    }

    pub const fn counts(self) -> [u8; PIECE_KIND_COUNT] {
        self.counts
    }

    pub const fn count(self, piece: PieceKind) -> u8 {
        self.counts[piece_index(piece)]
    }

    pub fn parse(value: &str) -> Result<Self, SpinStructureError> {
        value
            .chars()
            .filter(|value| !value.is_ascii_whitespace() && *value != ',')
            .map(|value| {
                PieceKind::from_ascii(value).map_err(|_| SpinStructureError::UnknownPiece(value))
            })
            .collect::<Result<Vec<_>, _>>()
            .and_then(Self::from_pieces)
    }

    pub fn total(self) -> u16 {
        self.counts.iter().map(|count| u16::from(*count)).sum()
    }

    pub fn available(self) -> impl Iterator<Item = PieceKind> {
        PieceKind::STANDARD_TETROMINOES
            .into_iter()
            .filter(move |piece| self.count(*piece) != 0)
    }

    pub(crate) fn take(self, piece: PieceKind) -> Option<Self> {
        let index = piece_index(piece);
        if self.counts[index] == 0 {
            return None;
        }
        let mut next = self;
        next.counts[index] -= 1;
        Some(next)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SpinStructureMode {
    TSpins,
    TSpinsPlus,
    AllMini,
    AllMiniPlus,
    AllSpin,
    AllSpinPlus,
}

impl SpinStructureMode {
    pub const ALL: [Self; 6] = [
        Self::TSpins,
        Self::TSpinsPlus,
        Self::AllMini,
        Self::AllMiniPlus,
        Self::AllSpin,
        Self::AllSpinPlus,
    ];

    pub const fn profile(self) -> SpinProfile {
        SpinProfile::builtin(match self {
            Self::TSpins => SpinProfileId::TSpins,
            Self::TSpinsPlus => SpinProfileId::TSpinsPlus,
            Self::AllMini => SpinProfileId::AllMini,
            Self::AllMiniPlus => SpinProfileId::AllMiniPlus,
            Self::AllSpin => SpinProfileId::AllSpin,
            Self::AllSpinPlus => SpinProfileId::AllSpinPlus,
        })
    }

    pub const fn t_only(self) -> bool {
        matches!(self, Self::TSpins | Self::TSpinsPlus)
    }

    pub const fn plus(self) -> bool {
        matches!(
            self,
            Self::TSpinsPlus | Self::AllMiniPlus | Self::AllSpinPlus
        )
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "t-spin" | "t-spins" => Some(Self::TSpins),
            "t-spin-plus" | "t-spins-plus" => Some(Self::TSpinsPlus),
            "all-mini" => Some(Self::AllMini),
            "all-mini-plus" => Some(Self::AllMiniPlus),
            "all-spin" | "all-spins" => Some(Self::AllSpin),
            "all-spin-plus" | "all-spins-plus" => Some(Self::AllSpinPlus),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TSpins => "t-spins",
            Self::TSpinsPlus => "t-spins-plus",
            Self::AllMini => "all-mini",
            Self::AllMiniPlus => "all-mini-plus",
            Self::AllSpin => "all-spin",
            Self::AllSpinPlus => "all-spin-plus",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SpinLineRequirement {
    Any,
    Exact(u8),
    AtLeast(u8),
}

impl SpinLineRequirement {
    pub const fn accepts(self, lines: u8) -> bool {
        match self {
            Self::Any => true,
            Self::Exact(expected) => lines == expected,
            Self::AtLeast(minimum) => lines >= minimum,
        }
    }

    pub const fn lower_bound(self) -> u8 {
        match self {
            Self::Any => 0,
            Self::Exact(lines) | Self::AtLeast(lines) => lines,
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim().to_ascii_lowercase();
        if matches!(value.as_str(), "any" | "all") {
            return Some(Self::Any);
        }
        if let Some(minimum) = value.strip_suffix('+').and_then(|v| v.parse().ok()) {
            return Some(Self::AtLeast(minimum));
        }
        value.parse().ok().map(Self::Exact)
    }

    pub fn as_str(self) -> String {
        match self {
            Self::Any => "any".to_string(),
            Self::Exact(lines) => lines.to_string(),
            Self::AtLeast(lines) => format!("{lines}+"),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MinimalityPolicy {
    /// Keep every structure for which no accepted proper operation subset exists.
    #[default]
    SubsetMinimal,
    /// Keep the complete globally shortest accepting placement layer.
    MinimumPieceCount,
}

impl MinimalityPolicy {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().replace('_', "-").as_str() {
            "subset" | "subset-minimal" | "minimal" => Some(Self::SubsetMinimal),
            "piece-count" | "minimum-piece-count" | "shortest" => Some(Self::MinimumPieceCount),
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SubsetMinimal => "subset-minimal",
            Self::MinimumPieceCount => "minimum-piece-count",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinStructureQuery {
    /// Input snapshot. Completed rows are cleared and the remaining rows are
    /// compacted once at the public search boundary before target generation.
    pub initial_board: StructureBoard,
    pub height: u8,
    pub inventory: PieceInventory,
    pub mode: SpinStructureMode,
    pub line_requirement: SpinLineRequirement,
    /// Inclusive bottom of the logical line-fill window.
    pub fill_bottom: u8,
    /// Exclusive top of the logical line-fill window.
    pub fill_top: u8,
    pub rule_profile: RuleProfileId,
    pub max_placements: Option<u8>,
    pub minimality: MinimalityPolicy,
}

impl SpinStructureQuery {
    pub fn new(inventory: PieceInventory, mode: SpinStructureMode) -> Self {
        Self {
            initial_board: StructureBoard::EMPTY,
            height: 8,
            inventory,
            mode,
            line_requirement: SpinLineRequirement::AtLeast(1),
            fill_bottom: 0,
            fill_top: 5,
            rule_profile: RuleProfileId::SrsPlus,
            max_placements: None,
            minimality: MinimalityPolicy::SubsetMinimal,
        }
    }

    pub fn validate(&self) -> Result<(), SpinStructureError> {
        if !(4..=StructureBoard::MAX_HEIGHT).contains(&self.height) {
            return Err(SpinStructureError::InvalidHeight(self.height));
        }
        if self.initial_board.has_cells_at_or_above(self.height) {
            return Err(SpinStructureError::InitialBoardOutsideHeight);
        }
        if self.fill_bottom >= self.fill_top || self.fill_top > self.height {
            return Err(SpinStructureError::InvalidFillWindow {
                bottom: self.fill_bottom,
                top: self.fill_top,
            });
        }
        if self.line_requirement.lower_bound() > 4 {
            return Err(SpinStructureError::InvalidLineRequirement(
                self.line_requirement.lower_bound(),
            ));
        }
        let total = self.inventory.total();
        if total > u16::from(u8::MAX) {
            return Err(SpinStructureError::InventoryTooLarge(total));
        }
        if self.max_placements.is_some_and(|limit| limit == 0) {
            return Err(SpinStructureError::ZeroPlacementLimit);
        }
        if !matches!(
            self.rule_profile,
            RuleProfileId::SrsPlus
                | RuleProfileId::Srs
                | RuleProfileId::SrsX
                | RuleProfileId::Jstris180
                | RuleProfileId::NoKick
        ) {
            return Err(SpinStructureError::UnsupportedRuleProfile(
                self.rule_profile,
            ));
        }
        Ok(())
    }

    pub fn placement_limit(&self) -> u8 {
        self.max_placements
            .unwrap_or_else(|| self.inventory.total().min(u16::from(u8::MAX)) as u8)
            .min(self.inventory.total().min(u16::from(u8::MAX)) as u8)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StructurePlacement {
    pub piece: PieceKind,
    pub rotation: RotationState,
    pub x: i8,
    pub y: i8,
    pub mask_before_clear: StructureBoard,
    pub cleared_rows: u32,
    pub cleared_lines: u8,
    pub evidence: ScoringLockEvidence,
}

/// A lock projected into the immutable, un-compacted structure coordinate
/// system. Physical witness coordinates remain available separately through
/// [`StructurePlacement`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StructureOperation {
    piece: PieceKind,
    rotation: RotationState,
    x: i8,
    y: i8,
    mask: StructureBoard,
    need_deleted_rows: u32,
}

impl StructureOperation {
    pub(crate) const fn new(
        piece: PieceKind,
        rotation: RotationState,
        x: i8,
        y: i8,
        mask: StructureBoard,
        need_deleted_rows: u32,
    ) -> Self {
        Self {
            piece,
            rotation,
            x,
            y,
            mask,
            need_deleted_rows,
        }
    }

    pub const fn piece(self) -> PieceKind {
        self.piece
    }

    pub const fn rotation(self) -> RotationState {
        self.rotation
    }

    pub const fn x(self) -> i8 {
        self.x
    }

    pub const fn y(self) -> i8 {
        self.y
    }

    pub const fn mask(self) -> StructureBoard {
        self.mask
    }

    pub const fn need_deleted_rows(self) -> u32 {
        self.need_deleted_rows
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinStructureOutcome {
    pub board_before_spin: StructureBoard,
    pub final_board: StructureBoard,
    pub spin: StructurePlacement,
    pub build: Vec<StructurePlacement>,
    pub mini: bool,
    pub(crate) logical_operations: Vec<StructureOperation>,
    pub(crate) logical_spin: StructureOperation,
    pub(crate) logical_spin_cleared_rows: u32,
}

impl SpinStructureOutcome {
    pub fn placement_count(&self) -> usize {
        self.build.len()
    }

    pub const fn is_mini(&self) -> bool {
        self.mini
    }

    pub fn logical_operations(&self) -> &[StructureOperation] {
        &self.logical_operations
    }

    pub const fn logical_spin(&self) -> StructureOperation {
        self.logical_spin
    }

    /// Immutable logical rows completed by the terminal target lock.
    pub const fn logical_spin_cleared_rows(&self) -> u32 {
        self.logical_spin_cleared_rows
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LayerMetrics {
    pub depth: u8,
    pub input_states: u64,
    pub piece_choices: u64,
    pub reachable_locks: u64,
    pub generated_states: u64,
    pub exact_duplicates: u64,
    pub terminal_candidates: u64,
    pub accepted_regular: u64,
    pub accepted_mini: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SpinStructureStageMetrics {
    pub build_states: u64,
    pub fill_checks: u64,
    pub support_locks: u64,
    pub corner_checks: u64,
    pub entry_states: u64,
    pub verification_checks: u64,
    pub exact_state_deduplications: u64,
    pub exact_outcome_deduplications: u64,
}

/// Coarse monotonic timings kept outside search hot loops.
///
/// Stage values are summed CPU-work time across target partitions. Per-depth
/// values use one clock sample at each layer boundary, not one sample per
/// state, so enabling the fixed benchmark surface has negligible search
/// overhead.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SpinStructureTimingMetrics {
    pub fill_ns: u64,
    pub expansion_ns: u64,
    pub finalization_ns: u64,
    pub layer_ns: [u64; 256],
}

impl Default for SpinStructureTimingMetrics {
    fn default() -> Self {
        Self {
            fill_ns: 0,
            expansion_ns: 0,
            finalization_ns: 0,
            layer_ns: [0; 256],
        }
    }
}

impl SpinStructureTimingMetrics {
    pub(crate) fn absorb(&mut self, other: Self) {
        self.fill_ns = self.fill_ns.saturating_add(other.fill_ns);
        self.expansion_ns = self.expansion_ns.saturating_add(other.expansion_ns);
        self.finalization_ns = self.finalization_ns.saturating_add(other.finalization_ns);
        for (left, right) in self.layer_ns.iter_mut().zip(other.layer_ns) {
            *left = left.saturating_add(right);
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SpinStructureReport {
    pub regular: Vec<SpinStructureOutcome>,
    pub mini: Vec<SpinStructureOutcome>,
    pub minimum_placements: Option<u8>,
    pub layers: Vec<LayerMetrics>,
    pub stages: SpinStructureStageMetrics,
    pub timings: SpinStructureTimingMetrics,
    pub workers_used: u16,
    pub complete: bool,
    pub query: Option<SpinStructureQuery>,
}

impl SpinStructureReport {
    pub fn outcomes(&self) -> impl Iterator<Item = &SpinStructureOutcome> {
        self.regular.iter().chain(&self.mini)
    }

    pub fn outcome_count(&self) -> usize {
        self.regular.len() + self.mini.len()
    }

    pub const fn workers_used(&self) -> u16 {
        self.workers_used
    }

    pub fn with_workers_used(mut self, workers_used: u16) -> Self {
        self.workers_used = workers_used.max(1);
        self
    }
}

/// An independent target-operation partition. Tasks share an immutable
/// catalog, can be moved to worker threads, and are merged deterministically
/// with [`SpinStructureSearcher::merge_task_reports`].
#[derive(Clone, Debug)]
pub struct SpinStructureTask {
    pub(crate) query: SpinStructureQuery,
    pub(crate) catalog: Arc<LogicalOperationCatalog>,
    pub(crate) target: StructureOperation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpinStructureError {
    InvalidHeight(u8),
    InitialBoardOutsideHeight,
    InvalidLineRequirement(u8),
    InventoryTooLarge(u16),
    PieceMultiplicityOverflow(PieceKind),
    UnknownPiece(char),
    ZeroPlacementLimit,
    InvalidFillWindow { bottom: u8, top: u8 },
    UnsupportedRuleProfile(RuleProfileId),
    IncompatibleTaskReports,
}

impl fmt::Display for SpinStructureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl Error for SpinStructureError {}

pub(crate) const fn piece_index(piece: PieceKind) -> usize {
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

/// Canonicalizes rotations whose occupied-cell geometry is identical.  Kick
/// evidence is retained on the placement itself; this projection is used only
/// for geometry-set identity and therefore cannot change terminal scoring.
pub(crate) const fn canonical_geometry_rotation(
    piece: PieceKind,
    rotation: RotationState,
) -> RotationState {
    match piece {
        PieceKind::O => RotationState::Zero,
        PieceKind::I | PieceKind::S | PieceKind::Z => match rotation {
            RotationState::Zero | RotationState::Two => RotationState::Zero,
            RotationState::Right | RotationState::Left => RotationState::Right,
        },
        PieceKind::T | PieceKind::J | PieceKind::L => rotation,
    }
}
