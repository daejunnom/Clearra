use super::{
    CBackendRequest, CBoardDescriptor, CBuildUpOperationSet, CCheckpointSpec, CPieceMultisetFamily,
    CPieceMultisetWindow, CPieceWindowDescriptor, CProblemBudget, CRuleProfileDescriptor,
    C_PIECE_SOURCE_PATTERN_READER_CAPACITY,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CPackingProblem {
    pub problem_kind: u32,
    pub max_pieces: u16,
    pub flags: u16,
    pub board: CBoardDescriptor,
    pub goal_region_mask: u64,
    pub required_fill_mask: u64,
    pub forbidden_mask: u64,
    pub exact_pieces: u16,
    pub reserved_goal: u16,
    pub piece_window: CPieceWindowDescriptor,
    pub piece_multiset_window: CPieceMultisetWindow,
    pub piece_multiset_family: CPieceMultisetFamily,
    pub piece_source: crate::supply::CPieceSourceDescriptor,
    pub piece_source_pattern_pieces: [u8; C_PIECE_SOURCE_PATTERN_READER_CAPACITY],
    pub piece_source_pattern_len: u16,
    pub piece_source_pattern_complete: u8,
    pub piece_source_pattern_reserved: u8,
    pub piece_source_pattern_truncation_reason: u16,
    pub piece_source_pattern_id: u32,
    pub rule: CRuleProfileDescriptor,
    pub budget: CProblemBudget,
    pub backend: CBackendRequest,
    pub checkpoint: CCheckpointSpec,
    pub goal: u32,
    pub count_policy: u32,
    pub objective: u32,
    pub label_count: u32,
}

impl Default for CPackingProblem {
    fn default() -> Self {
        Self {
            problem_kind: 0,
            max_pieces: 0,
            flags: 0,
            board: CBoardDescriptor::default(),
            goal_region_mask: 0,
            required_fill_mask: 0,
            forbidden_mask: 0,
            exact_pieces: 0,
            reserved_goal: 0,
            piece_window: CPieceWindowDescriptor::default(),
            piece_multiset_window: CPieceMultisetWindow::default(),
            piece_multiset_family: CPieceMultisetFamily::default(),
            piece_source: crate::supply::CPieceSourceDescriptor::default(),
            piece_source_pattern_pieces: [0; C_PIECE_SOURCE_PATTERN_READER_CAPACITY],
            piece_source_pattern_len: 0,
            piece_source_pattern_complete: 0,
            piece_source_pattern_reserved: 0,
            piece_source_pattern_truncation_reason: 0,
            piece_source_pattern_id: 0,
            rule: CRuleProfileDescriptor::default(),
            budget: CProblemBudget::default(),
            backend: CBackendRequest::default(),
            checkpoint: CCheckpointSpec::default(),
            goal: 0,
            count_policy: 0,
            objective: 0,
            label_count: 0,
        }
    }
}

impl CPackingProblem {
    pub const OPENING_PC: u32 = 1;
    pub const SCENARIO_PC: u32 = 2;
    pub const SETUP: u32 = 3;
    pub const BUILD: u32 = 4;
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CBuildUpProblem {
    pub packing: CPackingProblem,
    pub initial_board: CBoardDescriptor,
    pub operation_set: CBuildUpOperationSet,
    pub geometry_catalog: usize,
    pub candidate_id: u64,
    pub canonical_operation_set_id: u64,
    pub piece_source: crate::supply::CPieceSourceDescriptor,
    pub piece_source_pattern_pieces: [u8; C_PIECE_SOURCE_PATTERN_READER_CAPACITY],
    pub piece_source_pattern_len: u16,
    pub piece_source_pattern_complete: u8,
    pub piece_source_pattern_reserved: u8,
    pub piece_source_pattern_truncation_reason: u16,
    pub piece_source_pattern_id: u32,
    pub initial_hold_automaton: crate::supply::CHoldAutomatonStateDescriptor,
    pub rule: CRuleProfileDescriptor,
    pub line_clear_policy: u32,
    pub piece_window: CPieceWindowDescriptor,
    pub goal: u32,
    pub coverage_pattern_id: u32,
    pub buildup_flags: u32,
    pub source_execution_mode: u32,
    pub terminal_projection_policy_version: u16,
    pub terminal_projection_policy: u8,
    pub terminal_projection_reserved: u8,
}

impl Default for CBuildUpProblem {
    fn default() -> Self {
        Self {
            packing: CPackingProblem::default(),
            initial_board: CBoardDescriptor::default(),
            operation_set: CBuildUpOperationSet::default(),
            geometry_catalog: 0,
            candidate_id: 0,
            canonical_operation_set_id: 0,
            piece_source: crate::supply::CPieceSourceDescriptor::default(),
            piece_source_pattern_pieces: [0; C_PIECE_SOURCE_PATTERN_READER_CAPACITY],
            piece_source_pattern_len: 0,
            piece_source_pattern_complete: 0,
            piece_source_pattern_reserved: 0,
            piece_source_pattern_truncation_reason: 0,
            piece_source_pattern_id: 0,
            initial_hold_automaton: crate::supply::CHoldAutomatonStateDescriptor::default(),
            rule: CRuleProfileDescriptor::default(),
            line_clear_policy: 0,
            piece_window: CPieceWindowDescriptor::default(),
            goal: 0,
            coverage_pattern_id: 0,
            buildup_flags: 0,
            source_execution_mode: super::C_BUILDUP_SOURCE_CONCRETE_PATTERN,
            terminal_projection_policy_version: super::C_BUILDUP_TERMINAL_PROJECTION_POLICY_VERSION,
            terminal_projection_policy: super::C_BUILDUP_TERMINAL_PROJECTION_DISABLED,
            terminal_projection_reserved: 0,
        }
    }
}
