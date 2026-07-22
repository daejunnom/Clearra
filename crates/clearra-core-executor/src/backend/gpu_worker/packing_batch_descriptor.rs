use clearra_core_ffi::{
    gpu::{
        CGpuPackingBatchDescriptorView, CGpuPackingBatchDescriptorViewError,
        CGpuPieceMultisetWindow,
    },
    problem::{C_PACKING_MAX_PIECES, C_PIECE_I, C_PIECE_L, C_PIECE_NONE},
};

use super::{PackingBatchId, PackingBatchValidationError};

mod c_descriptor_adapter {
    use super::*;

    impl PackingBatchDescriptor {
        pub fn to_c_descriptor_view(
            self,
        ) -> Result<CGpuPackingBatchDescriptorView, CGpuPackingBatchDescriptorViewError> {
            CGpuPackingBatchDescriptorView::new_with_runtime_limits(
                self.batch_id.get(),
                self.board_width,
                self.board_height,
                self.active_packing_rows,
                self.goal_clear_lines_hint.unwrap_or(0),
                self.piece_window,
                self.piece_count,
                self.exact_piece_count,
                self.piece_source_kind,
                self.piece_source_id,
                self.piece_multiset_window,
                self.initial_board_mask,
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
}
mod constructor {
    use super::*;

    impl PackingBatchDescriptor {
        #[allow(clippy::too_many_arguments)]
        pub fn new(
            batch_id: PackingBatchId,
            board_width: u8,
            board_height: u8,
            active_packing_rows: u8,
            goal_clear_lines_hint: Option<u8>,
            initial_board_mask: u64,
            piece_window: u8,
            piece_count: u8,
            exact_piece_count: u8,
            piece_source_kind: u8,
            piece_source_id: u64,
            piece_multiset_window: CGpuPieceMultisetWindow,
            operation_table_id: u64,
            rule_profile_id: u64,
            kick_profile_id: u64,
            candidate_capacity: u32,
            shape_hash_seed: u64,
            pattern_universe_id: u64,
            pattern_weight_model_id: u64,
        ) -> Result<Self, PackingBatchValidationError> {
            Self::new_with_runtime_limits(
                batch_id,
                board_width,
                board_height,
                active_packing_rows,
                goal_clear_lines_hint,
                initial_board_mask,
                piece_window,
                piece_count,
                exact_piece_count,
                piece_source_kind,
                piece_source_id,
                piece_multiset_window,
                operation_table_id,
                rule_profile_id,
                kick_profile_id,
                candidate_capacity,
                2_048,
                1,
                shape_hash_seed,
                pattern_universe_id,
                pattern_weight_model_id,
            )
        }
    }
    impl PackingBatchDescriptor {
        #[allow(clippy::too_many_arguments)]
        pub fn new_with_runtime_limits(
            batch_id: PackingBatchId,
            board_width: u8,
            board_height: u8,
            active_packing_rows: u8,
            goal_clear_lines_hint: Option<u8>,
            initial_board_mask: u64,
            piece_window: u8,
            piece_count: u8,
            exact_piece_count: u8,
            piece_source_kind: u8,
            piece_source_id: u64,
            piece_multiset_window: CGpuPieceMultisetWindow,
            operation_table_id: u64,
            rule_profile_id: u64,
            kick_profile_id: u64,
            candidate_capacity: u32,
            max_frontier_states: u32,
            pattern_count: u32,
            shape_hash_seed: u64,
            pattern_universe_id: u64,
            pattern_weight_model_id: u64,
        ) -> Result<Self, PackingBatchValidationError> {
            let descriptor = Self {
                batch_id,
                board_width,
                board_height,
                active_packing_rows,
                goal_clear_lines_hint,
                initial_board_mask,
                piece_window,
                piece_count,
                exact_piece_count,
                piece_source_kind,
                piece_source_id,
                piece_multiset_window,
                operation_table_id,
                rule_profile_id,
                kick_profile_id,
                candidate_capacity,
                max_frontier_states,
                pattern_count,
                shape_hash_seed,
                pattern_universe_id,
                pattern_weight_model_id,
            };
            descriptor.validate()?;
            Ok(descriptor)
        }
    }
}
mod identity {
    use super::*;

    impl PackingBatchDescriptor {
        pub const fn product_source_of_truth(self) -> (u64, u64, u64, CGpuPieceMultisetWindow) {
            (
                self.piece_source_id,
                self.pattern_universe_id,
                self.pattern_weight_model_id,
                self.piece_multiset_window,
            )
        }
    }
}
mod model {
    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct PackingBatchDescriptor {
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
}
mod multiset_validator {
    use super::*;

    pub(super) fn gpu_piece_multiset_window_is_valid(window: &CGpuPieceMultisetWindow) -> bool {
        if window.total_count == 0
            || usize::from(window.total_count) > C_PACKING_MAX_PIECES
            || window.exact_count > window.total_count
            || window.counts[usize::from(C_PIECE_NONE)] != 0
        {
            return false;
        }
        let counted = (C_PIECE_I..=C_PIECE_L)
            .map(|piece| u16::from(window.counts[usize::from(piece)]))
            .sum::<u16>();
        counted == u16::from(window.total_count)
    }
}
mod validator {
    use super::{multiset_validator::gpu_piece_multiset_window_is_valid, *};

    impl PackingBatchDescriptor {
        pub fn validate(self) -> Result<(), PackingBatchValidationError> {
            validate_board(self)?;
            validate_piece_supply(self)?;
            validate_runtime_limits(self)?;
            validate_identity(self)
        }
    }

    fn validate_board(value: PackingBatchDescriptor) -> Result<(), PackingBatchValidationError> {
        if value.batch_id.get() == 0 {
            return Err(PackingBatchValidationError::ZeroBatchId);
        }
        if value.board_width == 0 || value.board_height == 0 {
            return Err(PackingBatchValidationError::ZeroBoardDimension);
        }
        let cell_count = u16::from(value.board_width) * u16::from(value.board_height);
        if cell_count > 64 {
            return Err(PackingBatchValidationError::BoardExceedsBoard64Limit { cell_count });
        }
        if value.active_packing_rows == 0 {
            return Err(PackingBatchValidationError::ZeroActivePackingRows);
        }
        if value.active_packing_rows > value.board_height {
            return Err(
                PackingBatchValidationError::ActivePackingRowsExceedBoardHeight {
                    active_packing_rows: value.active_packing_rows,
                    board_height: value.board_height,
                },
            );
        }
        if value
            .goal_clear_lines_hint
            .is_some_and(|hint| hint > value.board_height)
        {
            return Err(
                PackingBatchValidationError::GoalClearLinesHintExceedsBoardHeight {
                    goal_clear_lines_hint: value.goal_clear_lines_hint.unwrap_or_default(),
                    board_height: value.board_height,
                },
            );
        }
        let active_cells = u16::from(value.board_width) * u16::from(value.active_packing_rows);
        let low_mask = if active_cells >= 64 {
            u64::MAX
        } else {
            (1u64 << active_cells) - 1
        };
        if value.initial_board_mask & !low_mask != 0 {
            return Err(PackingBatchValidationError::InitialBoardMaskOutsideActivePackingRows);
        }
        Ok(())
    }

    fn validate_piece_supply(
        value: PackingBatchDescriptor,
    ) -> Result<(), PackingBatchValidationError> {
        if value.piece_window == 0 {
            return Err(PackingBatchValidationError::ZeroPieceWindow);
        }
        if value.piece_count == 0 {
            return Err(PackingBatchValidationError::ZeroPieceCount);
        }
        if value.piece_count > value.piece_window {
            return Err(PackingBatchValidationError::PieceCountExceedsPieceWindow {
                piece_count: value.piece_count,
                piece_window: value.piece_window,
            });
        }
        if value.exact_piece_count > value.piece_window {
            return Err(
                PackingBatchValidationError::ExactPieceCountExceedsPieceWindow {
                    exact_piece_count: value.exact_piece_count,
                    piece_window: value.piece_window,
                },
            );
        }
        if !(1..=3).contains(&value.piece_source_kind) {
            return Err(PackingBatchValidationError::UnknownPieceSourceKind {
                piece_source_kind: value.piece_source_kind,
            });
        }
        if value.piece_source_id == 0 {
            return Err(PackingBatchValidationError::MissingPieceSourceId);
        }
        if !gpu_piece_multiset_window_is_valid(&value.piece_multiset_window) {
            return Err(PackingBatchValidationError::InvalidPieceMultisetWindow);
        }
        if value.piece_count != value.piece_multiset_window.total_count {
            return Err(
                PackingBatchValidationError::PieceCountDoesNotMatchMultiset {
                    piece_count: value.piece_count,
                    total_count: value.piece_multiset_window.total_count,
                },
            );
        }
        if value.piece_multiset_window.exact_count != 0
            && value.exact_piece_count != value.piece_multiset_window.exact_count
        {
            return Err(
                PackingBatchValidationError::ExactPieceCountDoesNotMatchMultiset {
                    exact_piece_count: value.exact_piece_count,
                    exact_count: value.piece_multiset_window.exact_count,
                },
            );
        }
        Ok(())
    }

    fn validate_runtime_limits(
        value: PackingBatchDescriptor,
    ) -> Result<(), PackingBatchValidationError> {
        if value.candidate_capacity == 0 {
            return Err(PackingBatchValidationError::ZeroCandidateCapacity);
        }
        if value.max_frontier_states == 0 {
            return Err(PackingBatchValidationError::ZeroMaxFrontierStates);
        }
        if value.pattern_count == 0 {
            return Err(PackingBatchValidationError::ZeroPatternCount);
        }
        Ok(())
    }

    fn validate_identity(value: PackingBatchDescriptor) -> Result<(), PackingBatchValidationError> {
        if value.rule_profile_id == 0 {
            return Err(PackingBatchValidationError::MissingRuleProfileId);
        }
        if value.kick_profile_id == 0 {
            return Err(PackingBatchValidationError::MissingKickProfileId);
        }
        if value.pattern_universe_id == 0 {
            return Err(PackingBatchValidationError::MissingPatternUniverseIdentity);
        }
        if value.pattern_weight_model_id == 0 {
            return Err(PackingBatchValidationError::MissingPatternWeightModelIdentity);
        }
        Ok(())
    }
}

pub use model::PackingBatchDescriptor;
