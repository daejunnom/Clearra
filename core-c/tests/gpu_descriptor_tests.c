#include "gpu_test_support.h"
void c_gpu_batch_descriptor_preserves_pattern_universe_id(void) {
    ClearraGpuPackingBatchDescriptor batch = c_abi_batch_descriptor();

    EXPECT_U64(batch.pattern_universe_id, 1001);
}void c_gpu_batch_descriptor_preserves_weight_model_id(void) {
    ClearraGpuPackingBatchDescriptor batch = c_abi_batch_descriptor();

    EXPECT_U64(batch.pattern_weight_model_id, 2001);
}void c_gpu_batch_descriptor_preserves_piece_window_exact_count_and_source(void) {
    ClearraGpuPackingBatchDescriptor batch = c_abi_batch_descriptor();

    EXPECT_U64(batch.piece_window, 5);
    EXPECT_U64(batch.piece_count, 5);
    EXPECT_U64(batch.exact_piece_count, 5);
    EXPECT_U64(batch.piece_source_kind, CLEARRA_GPU_PIECE_SOURCE_FIXED_SEQUENCE);
}void gpu_batch_descriptor_has_piece_source_id(void) {
    ClearraGpuPackingBatchDescriptor batch = c_abi_batch_descriptor();

    EXPECT_U64(batch.piece_source_id, 9001);
}void gpu_batch_descriptor_has_piece_multiset_window(void) {
    ClearraGpuPackingBatchDescriptor batch = c_abi_batch_descriptor();
    clr_gpu_piece_multiset_window window;

    EXPECT_GPU_STATUS(clearra_gpu_batch_descriptor_piece_multiset_window(&batch, &window),
                      CLEARRA_GPU_OK);

    EXPECT_U64(window.total_count, 5);
    EXPECT_U64(window.exact_count, 5);
    EXPECT_U64(window.counts[CLR_PIECE_I], 1);
    EXPECT_U64(window.counts[CLR_PIECE_O], 1);
    EXPECT_U64(window.counts[CLR_PIECE_T], 1);
    EXPECT_U64(window.counts[CLR_PIECE_S], 1);
    EXPECT_U64(window.counts[CLR_PIECE_Z], 1);
}void c_gpu_batch_descriptor_product_source_of_truth_is_source_and_multiset(void) {
    ClearraGpuPackingBatchDescriptor batch = c_abi_batch_descriptor();
    clr_gpu_piece_multiset_window window;
    uint64_t piece_source_id = 0u;
    uint64_t pattern_universe_id = 0u;
    uint64_t pattern_weight_model_id = 0u;

    EXPECT_GPU_STATUS(clearra_gpu_batch_descriptor_product_source_of_truth(
                          &batch,
                          &piece_source_id,
                          &pattern_universe_id,
                          &pattern_weight_model_id,
                          &window),
                      CLEARRA_GPU_OK);

    EXPECT_U64(piece_source_id, 9001u);
    EXPECT_U64(pattern_universe_id, 1001u);
    EXPECT_U64(pattern_weight_model_id, 2001u);
    EXPECT_U64(window.total_count, 5u);
    EXPECT_U64(window.counts[CLR_PIECE_I], 1u);
}void c_gpu_batch_descriptor_preserves_active_rows_and_clear_hint(void) {
    ClearraGpuPackingBatchDescriptor batch = c_abi_batch_descriptor();

    EXPECT_U64(batch.active_packing_rows, 2);
    EXPECT_U64(batch.goal_clear_lines_hint, 0);
}void gpu_batch_descriptor_abi_size_is_stable(void) {
    EXPECT_U64(CLEARRA_GPU_PACKING_BATCH_DESCRIPTOR_ABI_VERSION, 5);
    EXPECT_U64(sizeof(ClearraGpuPackingBatchDescriptor),
               CLEARRA_GPU_PACKING_BATCH_DESCRIPTOR_ABI_SIZE);
}void gpu_packing_batch_descriptor_is_primary_abi(void) {
    EXPECT_U64(sizeof(ClearraGpuPackingBatchDescriptor),
               sizeof(ClearraGpuPackingBatchDescriptor));
    EXPECT_U64(sizeof(ClearraGpuPackingBatchDescriptor),
               sizeof(ClearraGpuPackingBatchDescriptor));
    EXPECT_U64(sizeof(ClearraGpuPackingBatchDescriptor),
               CLEARRA_GPU_PACKING_BATCH_DESCRIPTOR_ABI_SIZE);
}void gpu_batch_descriptor_rejects_unsupported_board_shape(void) {
    ClearraGpuPackingBatchDescriptor batch = c_abi_batch_descriptor();
    batch.board_height = 7;
    batch.active_packing_rows = 7;

    EXPECT_GPU_STATUS(clearra_gpu_batch_descriptor_validate(&batch),
                      CLEARRA_GPU_INVALID_ARGUMENT);
}void gpu_batch_descriptor_rejects_active_rows_over_board_height(void) {
    ClearraGpuPackingBatchDescriptor batch = c_abi_batch_descriptor();
    batch.active_packing_rows = 3;

    EXPECT_GPU_STATUS(clearra_gpu_batch_descriptor_validate(&batch),
                      CLEARRA_GPU_INVALID_ARGUMENT);
}void gpu_batch_descriptor_rejects_clear_hint_over_board_height(void) {
    ClearraGpuPackingBatchDescriptor batch = c_abi_batch_descriptor();
    batch.goal_clear_lines_hint = 3;

    EXPECT_GPU_STATUS(clearra_gpu_batch_descriptor_validate(&batch),
                      CLEARRA_GPU_INVALID_ARGUMENT);
}void gpu_batch_descriptor_rejects_exact_piece_count_over_window(void) {
    ClearraGpuPackingBatchDescriptor batch = c_abi_batch_descriptor();
    batch.exact_piece_count = 6;

    EXPECT_GPU_STATUS(clearra_gpu_batch_descriptor_validate(&batch),
                      CLEARRA_GPU_INVALID_ARGUMENT);
}void gpu_batch_descriptor_rejects_piece_count_exceeding_window(void) {
    ClearraGpuPackingBatchDescriptor batch = c_abi_batch_descriptor();
    batch.piece_window = 4;

    EXPECT_GPU_STATUS(clearra_gpu_batch_descriptor_validate(&batch),
                      CLEARRA_GPU_INVALID_ARGUMENT);
}void gpu_batch_descriptor_rejects_mask_outside_active_packing_rows(void) {
    ClearraGpuPackingBatchDescriptor batch = c_abi_batch_descriptor();
    batch.board_height = 4;
    batch.active_packing_rows = 2;
    batch.initial_board_mask = UINT64_C(1) << 20;

    EXPECT_GPU_STATUS(clearra_gpu_batch_descriptor_validate(&batch),
                      CLEARRA_GPU_INVALID_ARGUMENT);
}void gpu_batch_descriptor_rejects_unknown_piece_source_kind(void) {
    ClearraGpuPackingBatchDescriptor batch = c_abi_batch_descriptor();
    batch.piece_source_kind = CLEARRA_GPU_PIECE_SOURCE_UNKNOWN;

    EXPECT_GPU_STATUS(clearra_gpu_batch_descriptor_validate(&batch),
                      CLEARRA_GPU_INVALID_ARGUMENT);
}static ClearraBoard64Layout four_wide_one_row_layout(void) {
    ClearraBoard64Layout layout;
    if (clearra_board64_make_layout(4, 1, &layout) != CLEARRA_BOARD64_OK) {
        fprintf(stderr, "failed to create 4x1 layout\n");
        exit(1);
    }
    return layout;
}static ClearraGpuPackingBatchDescriptor multiset_i_batch(void) {
    ClearraGpuPackingBatchDescriptor batch;
    uint8_t piece = CLR_PIECE_I;
    EXPECT_GPU_STATUS(clearra_gpu_batch_descriptor_init(
                          four_wide_one_row_layout(),
                          0u,
                          1u,
                          &piece,
                          1u,
                          &batch),
                      CLEARRA_GPU_OK);

    EXPECT_GPU_STATUS(clearra_gpu_batch_descriptor_validate(&batch),
                      CLEARRA_GPU_OK);
    return batch;
}void cpu_packing_reference_uses_multiset_window(void) {
    ClearraGpuPackingBatchDescriptor batch = multiset_i_batch();
    static ClearraPackingCandidateBuffer output;

    EXPECT_GPU_STATUS(clearra_cpu_packing_reference_generate(&batch, &output),
                      CLEARRA_GPU_OK);

    EXPECT_TRUE(output.count > 0u);
    EXPECT_U64(output.placed_counts[0], 1u);
    EXPECT_U64(output.pieces[0][0], CLR_PIECE_I);
}void standard_gpu_descriptor_unchanged(void) {
    EXPECT_U64(sizeof(ClearraGpuPackingBatchDescriptor),
               sizeof(ClearraGpuPackingBatchDescriptor));
    EXPECT_U64(sizeof(ClearraGpuPackingBatchDescriptor),
               sizeof(ClearraGpuPackingBatchDescriptor));
    EXPECT_U64(sizeof(ClearraGpuPackingBatchDescriptor),
               CLEARRA_GPU_PACKING_BATCH_DESCRIPTOR_ABI_SIZE);
}
