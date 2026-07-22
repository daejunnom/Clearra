use crate::problem::{
    C_GPU_PIECE_SOURCE_BAG_ALIGNED_PATTERN, C_GPU_PIECE_SOURCE_FIXED_SEQUENCE,
    C_GPU_PIECE_SOURCE_OBSERVED_WINDOW, C_PACKING_MAX_PIECES, C_PIECE_I, C_PIECE_L, C_PIECE_NONE,
};

mod constructor {
    use super::*;

    impl CGpuPackingBatchDescriptorView {
        #[allow(clippy::too_many_arguments)]
        pub fn new(
            batch_id: u64,
            board_width: u8,
            board_height: u8,
            active_packing_rows: u8,
            goal_clear_lines_hint: u8,
            piece_window: u8,
            piece_count: u8,
            exact_piece_count: u8,
            piece_source_kind: u8,
            piece_source_id: u64,
            piece_multiset_window: CGpuPieceMultisetWindow,
            initial_board_mask: u64,
            operation_table_id: u64,
            rule_profile_id: u64,
            kick_profile_id: u64,
            candidate_capacity: u32,
            shape_hash_seed: u64,
            pattern_universe_id: u64,
            pattern_weight_model_id: u64,
        ) -> Result<Self, CGpuPackingBatchDescriptorViewError> {
            Self::new_with_runtime_limits(
                batch_id,
                board_width,
                board_height,
                active_packing_rows,
                goal_clear_lines_hint,
                piece_window,
                piece_count,
                exact_piece_count,
                piece_source_kind,
                piece_source_id,
                piece_multiset_window,
                initial_board_mask,
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
    impl CGpuPackingBatchDescriptorView {
        #[allow(clippy::too_many_arguments)]
        pub fn new_with_runtime_limits(
            batch_id: u64,
            board_width: u8,
            board_height: u8,
            active_packing_rows: u8,
            goal_clear_lines_hint: u8,
            piece_window: u8,
            piece_count: u8,
            exact_piece_count: u8,
            piece_source_kind: u8,
            piece_source_id: u64,
            piece_multiset_window: CGpuPieceMultisetWindow,
            initial_board_mask: u64,
            operation_table_id: u64,
            rule_profile_id: u64,
            kick_profile_id: u64,
            candidate_capacity: u32,
            max_frontier_states: u32,
            pattern_count: u32,
            shape_hash_seed: u64,
            pattern_universe_id: u64,
            pattern_weight_model_id: u64,
        ) -> Result<Self, CGpuPackingBatchDescriptorViewError> {
            let view = Self {
                batch_id,
                board_width,
                board_height,
                active_packing_rows,
                goal_clear_lines_hint,
                piece_window,
                piece_count,
                exact_piece_count,
                piece_source_kind,
                piece_source_id,
                piece_multiset_window,
                initial_board_mask,
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
            view.validate()?;
            Ok(view)
        }
    }
}
mod debug_snapshot {
    use super::CGpuPieceMultisetWindow;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct CGpuPackingBatchDescriptorDebugSnapshot {
        pub batch_id: u64,
        pub board_width: u8,
        pub board_height: u8,
        pub active_packing_rows: u8,
        pub goal_clear_lines_hint: u8,
        pub piece_window: u8,
        pub piece_count: u8,
        pub exact_piece_count: u8,
        pub piece_source_kind: u8,
        pub piece_source_id: u64,
        pub piece_multiset_window: CGpuPieceMultisetWindow,
        pub initial_board_mask: u64,
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
mod descriptor_view {
    use super::CGpuPieceMultisetWindow;

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct CGpuPackingBatchDescriptorView {
        pub batch_id: u64,
        pub board_width: u8,
        pub board_height: u8,
        pub active_packing_rows: u8,
        pub goal_clear_lines_hint: u8,
        pub piece_window: u8,
        pub piece_count: u8,
        pub exact_piece_count: u8,
        pub piece_source_kind: u8,
        pub piece_source_id: u64,
        pub piece_multiset_window: CGpuPieceMultisetWindow,
        pub initial_board_mask: u64,
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

    impl CGpuPackingBatchDescriptorView {
        pub const ABI_SIZE: usize = 112;
        pub const ABI_ALIGN: usize = 8;
    }
}
mod error {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum CGpuPackingBatchDescriptorViewError {
        ZeroBatchId,
        ZeroBoardDimension,
        UnsupportedBoardShape {
            cell_count: u16,
        },
        ZeroActivePackingRows,
        ActivePackingRowsExceedBoardHeight,
        GoalClearLinesHintExceedsBoardHeight,
        ZeroPieceWindow,
        ZeroPieceCount,
        PieceCountExceedsPieceWindow {
            piece_count: u8,
            piece_window: u8,
        },
        ExactPieceCountExceedsPieceWindow {
            exact_piece_count: u8,
            piece_window: u8,
        },
        UnknownPieceSourceKind,
        MissingPieceSourceId,
        InvalidPieceMultisetWindow,
        PieceCountDoesNotMatchMultiset {
            piece_count: u8,
            total_count: u8,
        },
        ExactPieceCountDoesNotMatchMultiset {
            exact_piece_count: u8,
            exact_count: u8,
        },
        ZeroCandidateCapacity,
        ZeroMaxFrontierStates,
        ZeroPatternCount,
        MissingOperationTableId,
        MissingRuleProfileId,
        MissingKickProfileId,
        MissingShapeHashSeed,
        MissingPatternUniverseIdentity,
        MissingPatternWeightModelIdentity,
        InitialBoardMaskOutsideActivePackingRows,
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
mod piece_multiset_window {
    use crate::problem::C_PIECE_L;

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct CGpuPieceMultisetWindow {
        pub counts: [u8; C_PIECE_L as usize + 1],
        pub total_count: u8,
        pub exact_count: u8,
        pub reserved: [u8; 6],
    }
}
mod product_identity {
    use super::*;

    impl CGpuPackingBatchDescriptorView {
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
mod snapshot_builder {
    use super::*;

    impl CGpuPackingBatchDescriptorView {
        pub fn debug_snapshot(self) -> CGpuPackingBatchDescriptorDebugSnapshot {
            CGpuPackingBatchDescriptorDebugSnapshot {
                batch_id: self.batch_id,
                board_width: self.board_width,
                board_height: self.board_height,
                active_packing_rows: self.active_packing_rows,
                goal_clear_lines_hint: self.goal_clear_lines_hint,
                piece_window: self.piece_window,
                piece_count: self.piece_count,
                exact_piece_count: self.exact_piece_count,
                piece_source_kind: self.piece_source_kind,
                piece_source_id: self.piece_source_id,
                piece_multiset_window: self.piece_multiset_window,
                initial_board_mask: self.initial_board_mask,
                operation_table_id: self.operation_table_id,
                rule_profile_id: self.rule_profile_id,
                kick_profile_id: self.kick_profile_id,
                candidate_capacity: self.candidate_capacity,
                max_frontier_states: self.max_frontier_states,
                pattern_count: self.pattern_count,
                shape_hash_seed: self.shape_hash_seed,
                pattern_universe_id: self.pattern_universe_id,
                pattern_weight_model_id: self.pattern_weight_model_id,
            }
        }
    }
}
mod validator {
    use super::{multiset_validator::gpu_piece_multiset_window_is_valid, *};

    impl CGpuPackingBatchDescriptorView {
        pub fn validate(self) -> Result<(), CGpuPackingBatchDescriptorViewError> {
            validate_board(self)?;
            validate_piece_window(self)?;
            validate_piece_source(self)?;
            validate_runtime_limits(self)?;
            validate_identity(self)?;
            validate_initial_mask(self)
        }
    }

    fn validate_board(
        view: CGpuPackingBatchDescriptorView,
    ) -> Result<(), CGpuPackingBatchDescriptorViewError> {
        if view.batch_id == 0 {
            return Err(CGpuPackingBatchDescriptorViewError::ZeroBatchId);
        }
        if view.board_width == 0 || view.board_height == 0 {
            return Err(CGpuPackingBatchDescriptorViewError::ZeroBoardDimension);
        }
        let cell_count = u16::from(view.board_width) * u16::from(view.board_height);
        if cell_count == 0 || cell_count > 64 {
            return Err(CGpuPackingBatchDescriptorViewError::UnsupportedBoardShape { cell_count });
        }
        if view.active_packing_rows == 0 {
            return Err(CGpuPackingBatchDescriptorViewError::ZeroActivePackingRows);
        }
        if view.active_packing_rows > view.board_height {
            return Err(CGpuPackingBatchDescriptorViewError::ActivePackingRowsExceedBoardHeight);
        }
        if view.goal_clear_lines_hint > view.board_height {
            return Err(CGpuPackingBatchDescriptorViewError::GoalClearLinesHintExceedsBoardHeight);
        }
        Ok(())
    }

    fn validate_piece_window(
        view: CGpuPackingBatchDescriptorView,
    ) -> Result<(), CGpuPackingBatchDescriptorViewError> {
        if view.piece_window == 0 {
            return Err(CGpuPackingBatchDescriptorViewError::ZeroPieceWindow);
        }
        if view.piece_count == 0 {
            return Err(CGpuPackingBatchDescriptorViewError::ZeroPieceCount);
        }
        if view.piece_count > view.piece_window {
            return Err(
                CGpuPackingBatchDescriptorViewError::PieceCountExceedsPieceWindow {
                    piece_count: view.piece_count,
                    piece_window: view.piece_window,
                },
            );
        }
        if view.exact_piece_count > view.piece_window {
            return Err(
                CGpuPackingBatchDescriptorViewError::ExactPieceCountExceedsPieceWindow {
                    exact_piece_count: view.exact_piece_count,
                    piece_window: view.piece_window,
                },
            );
        }
        Ok(())
    }

    fn validate_piece_source(
        view: CGpuPackingBatchDescriptorView,
    ) -> Result<(), CGpuPackingBatchDescriptorViewError> {
        if !matches!(
            view.piece_source_kind,
            C_GPU_PIECE_SOURCE_FIXED_SEQUENCE
                | C_GPU_PIECE_SOURCE_BAG_ALIGNED_PATTERN
                | C_GPU_PIECE_SOURCE_OBSERVED_WINDOW
        ) {
            return Err(CGpuPackingBatchDescriptorViewError::UnknownPieceSourceKind);
        }
        if view.piece_source_id == 0 {
            return Err(CGpuPackingBatchDescriptorViewError::MissingPieceSourceId);
        }
        if !gpu_piece_multiset_window_is_valid(&view.piece_multiset_window) {
            return Err(CGpuPackingBatchDescriptorViewError::InvalidPieceMultisetWindow);
        }
        if view.piece_count != view.piece_multiset_window.total_count {
            return Err(
                CGpuPackingBatchDescriptorViewError::PieceCountDoesNotMatchMultiset {
                    piece_count: view.piece_count,
                    total_count: view.piece_multiset_window.total_count,
                },
            );
        }
        if view.piece_multiset_window.exact_count != 0
            && view.exact_piece_count != view.piece_multiset_window.exact_count
        {
            return Err(
                CGpuPackingBatchDescriptorViewError::ExactPieceCountDoesNotMatchMultiset {
                    exact_piece_count: view.exact_piece_count,
                    exact_count: view.piece_multiset_window.exact_count,
                },
            );
        }
        Ok(())
    }

    fn validate_runtime_limits(
        view: CGpuPackingBatchDescriptorView,
    ) -> Result<(), CGpuPackingBatchDescriptorViewError> {
        if view.candidate_capacity == 0 {
            return Err(CGpuPackingBatchDescriptorViewError::ZeroCandidateCapacity);
        }
        if view.max_frontier_states == 0 {
            return Err(CGpuPackingBatchDescriptorViewError::ZeroMaxFrontierStates);
        }
        if view.pattern_count == 0 {
            return Err(CGpuPackingBatchDescriptorViewError::ZeroPatternCount);
        }
        Ok(())
    }

    fn validate_identity(
        view: CGpuPackingBatchDescriptorView,
    ) -> Result<(), CGpuPackingBatchDescriptorViewError> {
        if view.operation_table_id == 0 {
            return Err(CGpuPackingBatchDescriptorViewError::MissingOperationTableId);
        }
        if view.rule_profile_id == 0 {
            return Err(CGpuPackingBatchDescriptorViewError::MissingRuleProfileId);
        }
        if view.kick_profile_id == 0 {
            return Err(CGpuPackingBatchDescriptorViewError::MissingKickProfileId);
        }
        if view.shape_hash_seed == 0 {
            return Err(CGpuPackingBatchDescriptorViewError::MissingShapeHashSeed);
        }
        if view.pattern_universe_id == 0 {
            return Err(CGpuPackingBatchDescriptorViewError::MissingPatternUniverseIdentity);
        }
        if view.pattern_weight_model_id == 0 {
            return Err(CGpuPackingBatchDescriptorViewError::MissingPatternWeightModelIdentity);
        }
        Ok(())
    }

    fn validate_initial_mask(
        view: CGpuPackingBatchDescriptorView,
    ) -> Result<(), CGpuPackingBatchDescriptorViewError> {
        let active_cell_count = u16::from(view.board_width) * u16::from(view.active_packing_rows);
        let low_mask = if active_cell_count >= 64 {
            u64::MAX
        } else {
            (1u64 << active_cell_count) - 1
        };
        if view.initial_board_mask & !low_mask != 0 {
            return Err(
                CGpuPackingBatchDescriptorViewError::InitialBoardMaskOutsideActivePackingRows,
            );
        }
        Ok(())
    }
}

pub use debug_snapshot::CGpuPackingBatchDescriptorDebugSnapshot;
pub use descriptor_view::CGpuPackingBatchDescriptorView;
pub use error::CGpuPackingBatchDescriptorViewError;
pub use piece_multiset_window::CGpuPieceMultisetWindow;
