#![cfg(feature = "experimental-native-gpu")]

use clearra_core_ffi::{
    gpu::{
        CGpuPackingBatchDescriptorView, CGpuPackingBatchDescriptorViewError,
        CGpuPieceMultisetWindow,
    },
    problem::{
        C_GPU_PIECE_SOURCE_FIXED_SEQUENCE, C_PIECE_I, C_PIECE_O, C_PIECE_S, C_PIECE_T, C_PIECE_Z,
    },
};
use std::mem::{align_of, size_of};

fn mixed_multiset() -> CGpuPieceMultisetWindow {
    let mut window = CGpuPieceMultisetWindow {
        total_count: 5,
        exact_count: 5,
        ..Default::default()
    };
    for piece in [C_PIECE_I, C_PIECE_O, C_PIECE_T, C_PIECE_S, C_PIECE_Z] {
        window.counts[usize::from(piece)] += 1;
    }
    window
}

fn descriptor() -> CGpuPackingBatchDescriptorView {
    CGpuPackingBatchDescriptorView::new(
        7,
        10,
        2,
        2,
        0,
        5,
        5,
        5,
        C_GPU_PIECE_SOURCE_FIXED_SEQUENCE,
        9001,
        mixed_multiset(),
        0,
        11,
        1,
        3,
        64,
        17,
        1001,
        2001,
    )
    .expect("valid C GPU descriptor")
}

mod case_c_gpu_batch_descriptor_preserves_pattern_universe_id {
    use super::*;

    #[test]
    fn c_gpu_batch_descriptor_preserves_pattern_universe_id() {
        assert_eq!(descriptor().debug_snapshot().pattern_universe_id, 1001);
    }
}

mod case_c_gpu_batch_descriptor_preserves_weight_model_id {
    use super::*;

    #[test]
    fn c_gpu_batch_descriptor_preserves_weight_model_id() {
        assert_eq!(descriptor().debug_snapshot().pattern_weight_model_id, 2001);
    }
}

mod case_c_gpu_batch_descriptor_preserves_piece_window_exact_count_and_source {
    use super::*;

    #[test]
    fn c_gpu_batch_descriptor_preserves_piece_window_exact_count_and_source() {
        let snapshot = descriptor().debug_snapshot();

        assert_eq!(snapshot.piece_window, 5);
        assert_eq!(snapshot.piece_count, 5);
        assert_eq!(snapshot.exact_piece_count, 5);
        assert_eq!(
            snapshot.piece_source_kind,
            C_GPU_PIECE_SOURCE_FIXED_SEQUENCE
        );
    }
}

mod case_gpu_batch_descriptor_has_piece_source_id {
    use super::*;

    #[test]
    fn gpu_batch_descriptor_has_piece_source_id() {
        assert_eq!(descriptor().debug_snapshot().piece_source_id, 9001);
    }
}

mod case_gpu_batch_descriptor_has_piece_multiset_window {
    use super::*;

    #[test]
    fn gpu_batch_descriptor_has_piece_multiset_window() {
        let window = descriptor().debug_snapshot().piece_multiset_window;

        assert_eq!(window.total_count, 5);
        assert_eq!(window.exact_count, 5);
        assert_eq!(window.counts[usize::from(C_PIECE_I)], 1);
        assert_eq!(window.counts[usize::from(C_PIECE_O)], 1);
        assert_eq!(window.counts[usize::from(C_PIECE_T)], 1);
        assert_eq!(window.counts[usize::from(C_PIECE_S)], 1);
        assert_eq!(window.counts[usize::from(C_PIECE_Z)], 1);
    }
}

mod case_gpu_batch_descriptor_defaults_explicit_runtime_limits {
    use super::*;

    #[test]
    fn gpu_batch_descriptor_defaults_explicit_runtime_limits() {
        let view = descriptor();

        assert_eq!(view.max_frontier_states, 2_048);
        assert_eq!(view.pattern_count, 1);
    }
}

mod case_c_gpu_batch_descriptor_product_source_of_truth_is_source_and_multiset {
    use super::*;

    #[test]
    fn c_gpu_batch_descriptor_product_source_of_truth_is_source_and_multiset() {
        let (piece_source_id, pattern_universe_id, pattern_weight_model_id, window) =
            descriptor().product_source_of_truth();

        assert_eq!(piece_source_id, 9001);
        assert_eq!(pattern_universe_id, 1001);
        assert_eq!(pattern_weight_model_id, 2001);
        assert_eq!(window.total_count, 5);
        assert_eq!(window.counts[usize::from(C_PIECE_I)], 1);
    }
}

mod case_c_gpu_batch_descriptor_preserves_active_rows_and_clear_hint {
    use super::*;

    #[test]
    fn c_gpu_batch_descriptor_preserves_active_rows_and_clear_hint() {
        let snapshot = descriptor().debug_snapshot();

        assert_eq!(snapshot.active_packing_rows, 2);
        assert_eq!(snapshot.goal_clear_lines_hint, 0);
    }
}

mod case_gpu_batch_descriptor_abi_size_is_stable {
    use super::*;

    #[test]
    fn gpu_batch_descriptor_abi_size_is_stable() {
        assert_eq!(
            size_of::<CGpuPackingBatchDescriptorView>(),
            CGpuPackingBatchDescriptorView::ABI_SIZE
        );
        assert_eq!(
            align_of::<CGpuPackingBatchDescriptorView>(),
            CGpuPackingBatchDescriptorView::ABI_ALIGN
        );
    }
}

mod case_gpu_batch_descriptor_rejects_unsupported_board_shape {
    use super::*;

    #[test]
    fn gpu_batch_descriptor_rejects_unsupported_board_shape() {
        let result = CGpuPackingBatchDescriptorView::new(
            7,
            10,
            7,
            7,
            0,
            5,
            5,
            5,
            C_GPU_PIECE_SOURCE_FIXED_SEQUENCE,
            9001,
            mixed_multiset(),
            0,
            11,
            1,
            3,
            64,
            17,
            1001,
            2001,
        );

        assert_eq!(
            result,
            Err(CGpuPackingBatchDescriptorViewError::UnsupportedBoardShape { cell_count: 70 })
        );
    }
}

mod case_gpu_batch_descriptor_rejects_active_rows_over_board_height {
    use super::*;

    #[test]
    fn gpu_batch_descriptor_rejects_active_rows_over_board_height() {
        let result = CGpuPackingBatchDescriptorView::new(
            7,
            10,
            2,
            3,
            0,
            5,
            5,
            5,
            C_GPU_PIECE_SOURCE_FIXED_SEQUENCE,
            9001,
            mixed_multiset(),
            0,
            11,
            1,
            3,
            64,
            17,
            1001,
            2001,
        );

        assert_eq!(
            result,
            Err(CGpuPackingBatchDescriptorViewError::ActivePackingRowsExceedBoardHeight)
        );
    }
}

mod case_gpu_batch_descriptor_rejects_clear_hint_over_board_height {
    use super::*;

    #[test]
    fn gpu_batch_descriptor_rejects_clear_hint_over_board_height() {
        let result = CGpuPackingBatchDescriptorView::new(
            7,
            10,
            2,
            2,
            3,
            5,
            5,
            5,
            C_GPU_PIECE_SOURCE_FIXED_SEQUENCE,
            9001,
            mixed_multiset(),
            0,
            11,
            1,
            3,
            64,
            17,
            1001,
            2001,
        );

        assert_eq!(
            result,
            Err(CGpuPackingBatchDescriptorViewError::GoalClearLinesHintExceedsBoardHeight)
        );
    }
}

mod case_gpu_batch_descriptor_rejects_piece_count_exceeding_window {
    use super::*;

    #[test]
    fn gpu_batch_descriptor_rejects_piece_count_exceeding_window() {
        let result = CGpuPackingBatchDescriptorView::new(
            7,
            10,
            2,
            2,
            0,
            4,
            5,
            5,
            C_GPU_PIECE_SOURCE_FIXED_SEQUENCE,
            9001,
            mixed_multiset(),
            0,
            11,
            1,
            3,
            64,
            17,
            1001,
            2001,
        );

        assert_eq!(
            result,
            Err(
                CGpuPackingBatchDescriptorViewError::PieceCountExceedsPieceWindow {
                    piece_count: 5,
                    piece_window: 4,
                }
            )
        );
    }
}

mod case_gpu_batch_descriptor_rejects_mask_outside_active_packing_rows {
    use super::*;

    #[test]
    fn gpu_batch_descriptor_rejects_mask_outside_active_packing_rows() {
        let result = CGpuPackingBatchDescriptorView::new(
            7,
            10,
            4,
            2,
            0,
            5,
            5,
            5,
            C_GPU_PIECE_SOURCE_FIXED_SEQUENCE,
            9001,
            mixed_multiset(),
            1u64 << 20,
            11,
            1,
            3,
            64,
            17,
            1001,
            2001,
        );

        assert_eq!(
            result,
            Err(CGpuPackingBatchDescriptorViewError::InitialBoardMaskOutsideActivePackingRows)
        );
    }
}
