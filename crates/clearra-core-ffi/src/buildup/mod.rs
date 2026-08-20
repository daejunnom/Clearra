pub mod build_variant_view;
pub mod coverage_row_view;
pub mod kick_evidence_view;
pub mod trace_step_view;

pub use build_variant_view::{CBuildVariantView, CBuildVariantViewError};
pub use coverage_row_view::{
    CCoverageOverlapReport, CCoverageRowView, CPatternBitSet, OwnedCorePatternBitSetSnapshot,
    C_COVERAGE_MAX_PATTERNS, C_COVERAGE_MAX_WORDS, C_COVERAGE_ROW_KIND_BUILD,
    C_SCORE_MATRIX_CAPACITY_EXCEEDED, C_SPIN_COVERAGE_CAPACITY_EXCEEDED,
};
pub use kick_evidence_view::CKickEvidenceView;
pub use trace_step_view::{
    CBuildUpTraceStep, CReachabilityEvidenceView, C_BUILDUP_HOLD_BRANCH_CURRENT,
    C_BUILDUP_HOLD_BRANCH_RELEASE_HELD_AT_TERMINAL, C_BUILDUP_HOLD_BRANCH_STORE_CURRENT,
    C_BUILDUP_HOLD_BRANCH_SWAP_HELD,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CLineClearState {
    pub deleted_row_mask: u16,
    pub deleted_count: u8,
    pub reserved: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CBuildUpState {
    pub board_mask: u64,
    pub line_clear_state: CLineClearState,
    pub cursor: u16,
    pub hold_piece: u8,
    pub hold_empty: u8,
    pub placed_pieces: u16,
    pub cleared_lines: u8,
    pub reserved: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CBuildUpEventKind {
    Placement = 1,
    HoldSwap = 2,
    HoldStore = 3,
    LineClear = 4,
}

impl Default for CBuildUpEventKind {
    fn default() -> Self {
        Self::Placement
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CBuildUpEvent {
    pub kind: CBuildUpEventKind,
    pub piece: u8,
    pub rotation: u8,
    pub x: i16,
    pub y: i16,
    pub board_before: u64,
    pub board_after: u64,
    pub cleared_lines: u8,
    pub reserved: [u8; 7],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CResultReducerCounts {
    pub total_solution_count: u64,
    pub unique_solution_count: u64,
    pub retained_trace_count: u32,
    pub count_complete: u8,
    pub trace_retention_truncated: u8,
    pub reserved: [u8; 6],
}
