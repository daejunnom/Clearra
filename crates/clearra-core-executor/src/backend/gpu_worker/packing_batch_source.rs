use clearra_core_ffi::gpu::CGpuPieceMultisetWindow;
use clearra_core_ffi::CPackingProblem;
use clearra_problem::SearchProblem;

use super::{
    PackingBatchDescriptor, PackingBatchId, PackingBatchSourceError, PackingBatchValidationError,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackingBatchSource {
    pub batch_id: PackingBatchId,
    pub board_width: u8,
    pub board_height: u8,
    pub active_packing_rows: u8,
    pub goal_clear_lines_hint: Option<u8>,
    pub initial_board_mask: u64,
    pub piece_window: u8,
    pub piece_count: u8,
    pub exact_piece_count: u8,
    pub piece_source_kind: u8,
    pub piece_source_id: u64,
    pub piece_multiset_window: CGpuPieceMultisetWindow,
    pub operation_table_id: u64,
    pub rule_profile_id: u64,
    pub kick_profile_id: u64,
    pub candidate_capacity: u32,
    pub max_frontier_states: u32,
    pub pattern_count: u32,
    pub shape_hash_seed: u64,
    pub pattern_universe_id: u64,
    pub pattern_weight_model_id: u64,
}

impl PackingBatchSource {
    pub fn from_search_problem(
        problem: &SearchProblem,
        compact: &CPackingProblem,
        batch_id: Option<PackingBatchId>,
        pattern_universe_id: Option<u64>,
        pattern_weight_model_id: Option<u64>,
    ) -> Result<Self, PackingBatchSourceError> {
        super::packing_batch_from_problem::packing_batch_source_from_problem(
            problem,
            compact,
            batch_id,
            pattern_universe_id,
            pattern_weight_model_id,
        )
    }
}
impl PackingBatchSource {
    pub fn from_compact_problem_with_identity(
        compact: &CPackingProblem,
        batch_id: Option<PackingBatchId>,
        pattern_universe_id: u64,
        pattern_weight_model_id: u64,
        candidate_capacity_override: Option<u32>,
    ) -> Result<Self, PackingBatchSourceError> {
        super::packing_batch_from_candidate_region::packing_batch_source_from_candidate_region(
            compact,
            batch_id,
            pattern_universe_id,
            pattern_weight_model_id,
            candidate_capacity_override,
        )
    }
}
impl PackingBatchSource {
    pub fn into_descriptor(self) -> Result<PackingBatchDescriptor, PackingBatchValidationError> {
        PackingBatchDescriptor::new_with_runtime_limits(
            self.batch_id,
            self.board_width,
            self.board_height,
            self.active_packing_rows,
            self.goal_clear_lines_hint,
            self.initial_board_mask,
            self.piece_window,
            self.piece_count,
            self.exact_piece_count,
            self.piece_source_kind,
            self.piece_source_id,
            self.piece_multiset_window,
            self.operation_table_id,
            self.rule_profile_id,
            self.kick_profile_id,
            self.candidate_capacity,
            self.max_frontier_states,
            self.pattern_count,
            self.shape_hash_seed,
            self.pattern_universe_id,
            self.pattern_weight_model_id,
        )
    }
}
