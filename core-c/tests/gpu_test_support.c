#include "gpu_test_support.h"
ClearraBoard64Layout two_line_layout(void) {
    ClearraBoard64Layout layout;
    if (clearra_board64_make_layout(10, 2, &layout) != CLEARRA_BOARD64_OK) {
        fprintf(stderr, "failed to create 10x2 layout\n");
        exit(1);
    }
    return layout;
}ClearraGpuPackingBatchDescriptor standard_batch(void) {
    ClearraGpuPackingBatchDescriptor batch;
    const uint8_t pieces[1] = {
        CLR_PIECE_O,
    };
    const uint64_t active_region_mask = (UINT64_C(1) << 20) - UINT64_C(1);
    const uint64_t o_missing_region = UINT64_C(0x0000000000000c03);
    EXPECT_GPU_STATUS(clearra_gpu_batch_descriptor_init(
                          two_line_layout(),
                          active_region_mask & ~o_missing_region,
                          2,
                          pieces,
                          1,
                          &batch),
                      CLEARRA_GPU_OK);
    return batch;
}ClearraGpuPackingBatchDescriptor mixed_piece_batch(void) {
    ClearraGpuPackingBatchDescriptor batch;
    const uint8_t pieces[2] = {
        CLR_PIECE_I,
        CLR_PIECE_O,
    };
    const uint64_t active_region_mask = (UINT64_C(1) << 20) - UINT64_C(1);
    const uint64_t mixed_missing_region = UINT64_C(0x0000000000000c3f);
    EXPECT_GPU_STATUS(clearra_gpu_batch_descriptor_init(
                          two_line_layout(),
                          active_region_mask & ~mixed_missing_region,
                          2,
                          pieces,
                          2,
                          &batch),
                      CLEARRA_GPU_OK);
    return batch;
}ClearraGpuPackingBatchDescriptor collision_batch(void) {
    ClearraGpuPackingBatchDescriptor batch = standard_batch();
    batch.initial_board_mask = (UINT64_C(1) << 20) - UINT64_C(1);
    EXPECT_GPU_STATUS(clearra_gpu_batch_descriptor_validate(&batch),
                      CLEARRA_GPU_OK);
    return batch;
}uint64_t shape_hash_for(const ClearraPackingCandidateBuffer *buffer) {
    uint64_t hash = UINT64_C(1469598103934665603);
    for (uint16_t index = 0; index < buffer->count; index++) {
        hash = clearra_cache_key_mix_u64(hash, buffer->shape_keys[index]);
    }
    return hash;
}uint64_t tiling_hash_for(const ClearraPackingCandidateBuffer *buffer) {
    uint64_t hash = UINT64_C(1099511628211);
    for (uint16_t index = 0; index < buffer->count; index++) {
        hash = clearra_cache_key_mix_u64(hash, buffer->tiling_keys[index]);
    }
    return hash;
}uint64_t operation_set_hash_for(const ClearraPackingCandidateBuffer *buffer) {
    uint64_t hash = UINT64_C(7809847782465536322);
    for (uint16_t index = 0; index < buffer->count; index++) {
        hash = clearra_cache_key_mix_u64(hash, buffer->operation_set_keys[index]);
    }
    return hash;
}void cpu_reference_for_gpu_batch(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraPackingCandidateBuffer *out_reference) {
    clr_packing_problem problem;

    EXPECT_GPU_STATUS(clearra_gpu_batch_descriptor_to_packing_problem(batch, &problem),
                      CLEARRA_GPU_OK);
    EXPECT_PACKING_STATUS(clearra_packing_enumerator_cpu_generate_problem(
                              &problem, out_reference),
                          CLEARRA_PACKING_OK);
}void expect_candidate_buffers_match_canonical(
    const ClearraPackingCandidateBuffer *left,
    const ClearraPackingCandidateBuffer *right) {
    static ClearraCanonicalPackingTable left_table;
    static ClearraCanonicalPackingTable right_table;

    EXPECT_PACKING_STATUS(clearra_packing_host_reduce(left, &left_table),
                          CLEARRA_PACKING_OK);
    EXPECT_PACKING_STATUS(clearra_packing_host_reduce(right, &right_table),
                          CLEARRA_PACKING_OK);
    EXPECT_TRUE(clearra_packing_candidate_buffer_exactly_matches(
        &left_table.candidates, &right_table.candidates));
}void canonical_hashes_for(
    const ClearraPackingCandidateBuffer *buffer,
    uint64_t *out_shape_hash,
    uint64_t *out_tiling_hash,
    uint64_t *out_operation_set_hash) {
    static ClearraCanonicalPackingTable table;

    EXPECT_PACKING_STATUS(clearra_packing_host_reduce(buffer, &table),
                          CLEARRA_PACKING_OK);
    *out_shape_hash = shape_hash_for(&table.candidates);
    *out_tiling_hash = tiling_hash_for(&table.candidates);
    *out_operation_set_hash = operation_set_hash_for(&table.candidates);
}ClearraGpuPackingBatchDescriptor c_abi_batch_descriptor(void) {
    ClearraGpuPackingBatchDescriptor batch = {
        .batch_id = 7,
        .board_width = 10,
        .board_height = 2,
        .active_packing_rows = 2,
        .goal_clear_lines_hint = 0,
        .piece_window = 5,
        .piece_count = 5,
        .exact_piece_count = 5,
        .piece_source_kind = CLEARRA_GPU_PIECE_SOURCE_FIXED_SEQUENCE,
        .piece_source_id = 9001,
        .piece_multiset_window = {
            .total_count = 5,
            .exact_count = 5,
        },
        .initial_board_mask = 0,
        .operation_table_id = 11,
        .rule_profile_id = 1,
        .kick_profile_id = 3,
        .candidate_capacity = 64,
        .max_frontier_states = 2048,
        .pattern_count = 1,
        .shape_hash_seed = 17,
        .pattern_universe_id = 1001,
        .pattern_weight_model_id = 2001,
    };
    batch.piece_multiset_window.counts[CLR_PIECE_I] = 1u;
    batch.piece_multiset_window.counts[CLR_PIECE_O] = 1u;
    batch.piece_multiset_window.counts[CLR_PIECE_T] = 1u;
    batch.piece_multiset_window.counts[CLR_PIECE_S] = 1u;
    batch.piece_multiset_window.counts[CLR_PIECE_Z] = 1u;
    EXPECT_GPU_STATUS(clearra_gpu_batch_descriptor_validate(&batch),
                      CLEARRA_GPU_OK);
    return batch;
}
