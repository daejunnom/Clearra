#include "gpu_test_support.h"
void gpu_packing_candidate_count_matches_cpu_reference(void) {
    ClearraGpuPackingBatchDescriptor batch = standard_batch();
    static ClearraGpuPackingResult gpu_result;
    static ClearraPackingCandidateBuffer cpu_reference;
    clearra_gpu_packing_result_clear(&gpu_result);

    EXPECT_GPU_STATUS(clearra_cpu_packing_reference_run(
                          &batch, &gpu_result),
                      CLEARRA_GPU_OK);
    cpu_reference_for_gpu_batch(&batch, &cpu_reference);

    EXPECT_U64(gpu_result.raw_candidate_count, cpu_reference.count);
    EXPECT_U64(gpu_result.canonical_candidate_count,
               gpu_result.canonical_candidates.candidates.count);
}void gpu_packing_mixed_piece_candidate_count_matches_cpu_reference(void) {
    ClearraGpuPackingBatchDescriptor batch = mixed_piece_batch();
    static ClearraGpuPackingResult gpu_result;
    static ClearraPackingCandidateBuffer cpu_reference;
    static ClearraCanonicalPackingTable cpu_table;
    clearra_gpu_packing_result_clear(&gpu_result);

    EXPECT_GPU_STATUS(clearra_cpu_packing_reference_run(
                          &batch, &gpu_result),
                      CLEARRA_GPU_OK);
    cpu_reference_for_gpu_batch(&batch, &cpu_reference);
    EXPECT_PACKING_STATUS(clearra_packing_host_reduce(&cpu_reference, &cpu_table),
                          CLEARRA_PACKING_OK);

    EXPECT_TRUE(cpu_reference.count > 0);
    EXPECT_U64(gpu_result.readback_uncompressed_count, cpu_reference.count);
    EXPECT_U64(gpu_result.canonical_candidate_count, cpu_table.candidates.count);
    EXPECT_TRUE(clearra_packing_candidate_buffer_exactly_matches(
        &gpu_result.canonical_candidates.candidates, &cpu_table.candidates));
}void gpu_partial_reference_result_cannot_source_exact_probability(void) {
    ClearraGpuPackingBatchDescriptor batch;
    const uint8_t pieces[5] = {
        CLR_PIECE_I,
        CLR_PIECE_I,
        CLR_PIECE_O,
        CLR_PIECE_O,
        CLR_PIECE_O,
    };
    static ClearraGpuPackingResult gpu_result;
    ClearraGpuWorkerRequest request;
    static ClearraGpuWorkerResult worker_result;

    EXPECT_GPU_STATUS(clearra_gpu_batch_descriptor_init(
                          two_line_layout(), 0u, 2u, pieces, 5u, &batch),
                      CLEARRA_GPU_OK);
    batch.candidate_capacity = 1u;
    batch.max_frontier_states = 65536u;
    EXPECT_GPU_STATUS(clearra_gpu_batch_descriptor_validate(&batch),
                      CLEARRA_GPU_OK);

    clearra_gpu_packing_result_clear(&gpu_result);
    EXPECT_GPU_STATUS(clearra_cpu_packing_reference_run(
                          &batch, &gpu_result),
                      CLEARRA_GPU_OK);
    EXPECT_FALSE(gpu_result.result_complete);
    EXPECT_U64(gpu_result.truncation_reason,
               CLR_RESOURCE_TRUNCATION_CANDIDATE_BUDGET_EXCEEDED);
    EXPECT_TRUE(gpu_result.cpu_exact_confirmed);
    EXPECT_TRUE(gpu_result.cpu_reference_matched);

    request = (ClearraGpuWorkerRequest){
        .request_id = 7u,
        .batch = batch,
        .memory_ticket_id = 11u,
        .fence_epoch = 3u,
        .scope_epoch = 3u,
        .byte_budget = sizeof(batch),
        .cpu_confirm_required = 1u,
    };
    EXPECT_GPU_STATUS((ClearraGpuStatus)clearra_gpu_worker_run(
                          &request, &worker_result),
                      (ClearraGpuStatus)CLEARRA_GPU_WORKER_UNAVAILABLE);
    EXPECT_FALSE(worker_result.can_source_exact_probability);
}void gpu_result_passes_hash_exact_confirm(void) {
    ClearraGpuPackingBatchDescriptor batch = standard_batch();
    static ClearraGpuPackingResult gpu_result;
    clearra_gpu_packing_result_clear(&gpu_result);

    EXPECT_GPU_STATUS(clearra_cpu_packing_reference_run(
                          &batch, &gpu_result),
                      CLEARRA_GPU_OK);

    EXPECT_TRUE(gpu_result.hash_exact_confirmed);
}void gpu_candidate_requires_cpu_exact_confirm(void) {
    ClearraGpuPackingBatchDescriptor batch = standard_batch();
    static ClearraGpuPackingResult gpu_result;
    clearra_gpu_packing_result_clear(&gpu_result);

    EXPECT_GPU_STATUS(clearra_cpu_packing_reference_run(
                          &batch, &gpu_result),
                      CLEARRA_GPU_OK);

    EXPECT_TRUE(gpu_result.hash_exact_confirmed);
    EXPECT_TRUE(gpu_result.cpu_exact_confirmed);
    EXPECT_TRUE(gpu_result.cpu_reference_matched);
    EXPECT_TRUE(gpu_result.deterministic_result);
}void gpu_shape_hash_collision_requires_exact_compare(void) {
    ClearraGpuPackingBatchDescriptor batch = mixed_piece_batch();
    static ClearraGpuPackingResult gpu_result;
    uint64_t original_hash;
    uint8_t matched = 1u;
    uint64_t cpu_reference_hash = 0u;
    clearra_gpu_packing_result_clear(&gpu_result);

    EXPECT_GPU_STATUS(clearra_cpu_packing_reference_run(
                          &batch, &gpu_result),
                      CLEARRA_GPU_OK);
    EXPECT_TRUE(gpu_result.canonical_candidates.candidates.count > 0);
    original_hash = gpu_result.gpu_candidate_hash;

    gpu_result.canonical_candidates.candidates.final_boards[0] ^= UINT64_C(1);
    gpu_result.gpu_candidate_hash = original_hash;

    EXPECT_GPU_STATUS(clearra_gpu_cpu_exact_confirm_reference(
                          &batch, &gpu_result, &matched, &cpu_reference_hash),
                      CLEARRA_GPU_OK);
    EXPECT_FALSE(matched);
    EXPECT_TRUE(cpu_reference_hash != 0u);
}static void assert_cpu_exact_confirm_rejects_mutated_key(uint8_t key_kind) {
    ClearraGpuPackingBatchDescriptor batch = mixed_piece_batch();
    static ClearraGpuPackingResult gpu_result;
    uint8_t matched = 1u;
    uint64_t cpu_reference_hash = 0u;
    clearra_gpu_packing_result_clear(&gpu_result);

    EXPECT_GPU_STATUS(clearra_cpu_packing_reference_run(
                          &batch, &gpu_result),
                      CLEARRA_GPU_OK);
    EXPECT_TRUE(gpu_result.canonical_candidates.candidates.count > 0);

    if (key_kind == 0u) {
        gpu_result.canonical_candidates.candidates.operation_set_keys[0] ^=
            UINT64_C(0x101);
    } else if (key_kind == 1u) {
        gpu_result.canonical_candidates.candidates.shape_keys[0] ^= UINT64_C(0x101);
    } else {
        gpu_result.canonical_candidates.candidates.tiling_keys[0] ^= UINT64_C(0x101);
    }

    EXPECT_GPU_STATUS(clearra_gpu_cpu_exact_confirm_reference(
                          &batch, &gpu_result, &matched, &cpu_reference_hash),
                      CLEARRA_GPU_OK);
    EXPECT_FALSE(matched);
    EXPECT_TRUE(cpu_reference_hash != 0u);
}void gpu_cpu_exact_confirm_rejects_operation_shape_or_tiling_key_mismatch(void) {
    assert_cpu_exact_confirm_rejects_mutated_key(0u);
    assert_cpu_exact_confirm_rejects_mutated_key(1u);
    assert_cpu_exact_confirm_rejects_mutated_key(2u);
}void gpu_strengthening_reports_batch_prefilter_hash_and_compression(void) {
    ClearraGpuPackingBatchDescriptor batch = standard_batch();
    static ClearraGpuPackingResult gpu_result;
    clearra_gpu_packing_result_clear(&gpu_result);

    EXPECT_GPU_STATUS(clearra_cpu_packing_reference_run(
                          &batch, &gpu_result),
                      CLEARRA_GPU_OK);

    EXPECT_TRUE(gpu_result.larger_batch_planner_enabled);
    EXPECT_TRUE(gpu_result.planned_batch_count > 0);
    EXPECT_U64(gpu_result.batch_candidate_capacity, CLEARRA_PACKING_MAX_CANDIDATES);
    EXPECT_TRUE(gpu_result.dominance_prefilter_applied);
    EXPECT_TRUE(gpu_result.shape_union_mask_applied);
    EXPECT_TRUE(gpu_result.gpu_shape_union_mask.value != 0);
    EXPECT_TRUE(gpu_result.gpu_candidate_hash != 0);
    EXPECT_TRUE(gpu_result.cpu_reference_hash != 0);
    EXPECT_U64(gpu_result.gpu_candidate_hash, gpu_result.cpu_reference_hash);
    EXPECT_TRUE(gpu_result.readback_compressed);
    EXPECT_U64(gpu_result.readback_compressed_count,
               gpu_result.canonical_candidate_count);
    EXPECT_TRUE(gpu_result.cpu_exact_confirmed);
    EXPECT_TRUE(gpu_result.cpu_exact_confirm_optimized);
    EXPECT_TRUE(gpu_result.cpu_reference_matched);
}void gpu_result_is_deterministic_and_cpu_reference_confirmed(void) {
    ClearraGpuPackingBatchDescriptor batch = standard_batch();
    static ClearraGpuPackingResult left;
    static ClearraGpuPackingResult right;
    clearra_gpu_packing_result_clear(&left);
    clearra_gpu_packing_result_clear(&right);

    EXPECT_GPU_STATUS(clearra_cpu_packing_reference_run(
                          &batch, &left),
                      CLEARRA_GPU_OK);
    EXPECT_GPU_STATUS(clearra_cpu_packing_reference_run(
                          &batch, &right),
                      CLEARRA_GPU_OK);

    EXPECT_TRUE(left.deterministic_result);
    EXPECT_TRUE(right.deterministic_result);
    EXPECT_TRUE(left.cpu_reference_matched);
    EXPECT_U64(left.gpu_candidate_hash, left.cpu_reference_hash);
    EXPECT_U64(left.gpu_candidate_hash, right.gpu_candidate_hash);
    EXPECT_U64(left.cpu_reference_hash, right.cpu_reference_hash);
    EXPECT_U64(left.raw_candidate_count, right.raw_candidate_count);
    EXPECT_U64(left.canonical_candidate_count, right.canonical_candidate_count);
}void gpu_result_cpu_reference_matched_before_build_queue(void) {
    ClearraGpuPackingBatchDescriptor batch = mixed_piece_batch();
    static ClearraGpuPackingResult gpu_result;
    ClearraGpuConfirmedCandidateQueue queue;
    clearra_gpu_packing_result_clear(&gpu_result);

    EXPECT_GPU_STATUS(clearra_cpu_packing_reference_run(
                          &batch, &gpu_result),
                      CLEARRA_GPU_OK);

    EXPECT_TRUE(gpu_result.hash_exact_confirmed);
    EXPECT_TRUE(gpu_result.cpu_exact_confirmed);
    EXPECT_TRUE(gpu_result.cpu_reference_matched);
    EXPECT_TRUE(gpu_result.deterministic_result);
    EXPECT_TRUE(gpu_result.gpu_candidate_hash != 0u);
    EXPECT_U64(gpu_result.gpu_candidate_hash, gpu_result.cpu_reference_hash);
    EXPECT_U64(gpu_result.raw_candidate_count, gpu_result.raw_candidates.count);
    EXPECT_U64(gpu_result.canonical_candidate_count,
               gpu_result.canonical_candidates.candidates.count);

    EXPECT_GPU_STATUS(clearra_gpu_confirmed_candidate_queue_from_result(
                          &gpu_result, &queue),
                      CLEARRA_GPU_OK);
    EXPECT_TRUE(queue.can_enter_cpu_buildup_queue);
    EXPECT_FALSE(queue.can_create_coverage_row);
    EXPECT_FALSE(queue.candidate_is_solution);
}void gpu_shape_union_mask_matches_raw_candidate_shapes(void) {
    ClearraGpuPackingBatchDescriptor batch = standard_batch();
    static ClearraGpuPackingResult gpu_result;
    clearra_gpu_packing_result_clear(&gpu_result);

    EXPECT_GPU_STATUS(clearra_cpu_packing_reference_run(
                          &batch, &gpu_result),
                      CLEARRA_GPU_OK);

    uint64_t expected_or = 0;
    for (uint16_t index = 0; index < gpu_result.raw_candidates.count; index++) {
        expected_or |= gpu_result.raw_candidates.shape_masks[index];
    }
    EXPECT_U64(gpu_result.gpu_shape_union_mask.value, expected_or);
}void gpu_candidate_is_not_output_as_solution_before_buildup(void) {
    ClearraGpuPackingBatchDescriptor batch = standard_batch();
    static ClearraGpuPackingResult gpu_result;
    clearra_gpu_packing_result_clear(&gpu_result);

    EXPECT_GPU_STATUS(clearra_cpu_packing_reference_run(
                          &batch, &gpu_result),
                      CLEARRA_GPU_OK);

    EXPECT_FALSE(gpu_result.candidate_is_solution);
    EXPECT_TRUE(gpu_result.raw_candidate_count > 0);
}void gpu_candidate_is_not_solution_before_buildup(void) {
    ClearraGpuPackingBatchDescriptor batch = mixed_piece_batch();
    static ClearraGpuPackingResult gpu_result;
    clearra_gpu_packing_result_clear(&gpu_result);

    EXPECT_GPU_STATUS(clearra_cpu_packing_reference_run(
                          &batch, &gpu_result),
                      CLEARRA_GPU_OK);

    EXPECT_FALSE(gpu_result.candidate_is_solution);
    EXPECT_TRUE(gpu_result.raw_candidate_count > 0);
    EXPECT_TRUE(gpu_result.cpu_reference_matched);
}void gpu_raw_candidate_cannot_enter_buildup_queue(void) {
    ClearraGpuPackingBatchDescriptor batch = standard_batch();
    static ClearraPackingCandidateBuffer raw_candidates;

    cpu_reference_for_gpu_batch(&batch, &raw_candidates);

    EXPECT_TRUE(raw_candidates.count > 0);
    EXPECT_FALSE(clearra_gpu_raw_candidate_buffer_can_enter_buildup_queue(
        &raw_candidates));
    EXPECT_FALSE(clearra_gpu_raw_candidate_buffer_can_create_coverage_row(
        &raw_candidates));
}void gpu_confirmed_candidate_enters_buildup_queue(void) {
    ClearraGpuPackingBatchDescriptor batch = standard_batch();
    static ClearraGpuPackingResult gpu_result;
    ClearraGpuConfirmedCandidateQueue queue;
    ClearraPackingCandidateView first_candidate;
    clearra_gpu_packing_result_clear(&gpu_result);

    EXPECT_GPU_STATUS(clearra_cpu_packing_reference_run(
                          &batch, &gpu_result),
                      CLEARRA_GPU_OK);
    EXPECT_GPU_STATUS(clearra_gpu_confirmed_candidate_queue_from_result(
                          &gpu_result, &queue),
                      CLEARRA_GPU_OK);

    EXPECT_TRUE(queue.count > 0);
    EXPECT_TRUE(queue.table == &gpu_result.canonical_candidates);
    EXPECT_TRUE(queue.cpu_exact_confirmed);
    EXPECT_TRUE(queue.can_enter_cpu_buildup_queue);
    EXPECT_FALSE(queue.can_create_coverage_row);
    EXPECT_FALSE(queue.candidate_is_solution);
    EXPECT_GPU_STATUS(clearra_gpu_confirmed_candidate_queue_candidate_at(
                          &queue, 0, &first_candidate),
                      CLEARRA_GPU_OK);
}void gpu_confirmed_candidate_is_still_not_solution(void) {
    ClearraGpuPackingBatchDescriptor batch = standard_batch();
    static ClearraGpuPackingResult gpu_result;
    ClearraGpuConfirmedCandidateQueue queue;
    clearra_gpu_packing_result_clear(&gpu_result);

    EXPECT_GPU_STATUS(clearra_cpu_packing_reference_run(
                          &batch, &gpu_result),
                      CLEARRA_GPU_OK);
    EXPECT_GPU_STATUS(clearra_gpu_confirmed_candidate_queue_from_result(
                          &gpu_result, &queue),
                      CLEARRA_GPU_OK);

    EXPECT_TRUE(gpu_result.cpu_exact_confirmed);
    EXPECT_TRUE(gpu_result.cpu_reference_matched);
    EXPECT_FALSE(gpu_result.candidate_is_solution);
    EXPECT_FALSE(queue.candidate_is_solution);
    EXPECT_FALSE(queue.can_create_coverage_row);
}void confirmed_candidate_can_enter_buildup_queue(void) {
    ClearraGpuPackingBatchDescriptor batch = standard_batch();
    static ClearraGpuPackingResult gpu_result;
    ClearraGpuConfirmedCandidateQueue queue;
    clearra_gpu_packing_result_clear(&gpu_result);

    EXPECT_GPU_STATUS(clearra_cpu_packing_reference_run(
                          &batch, &gpu_result),
                      CLEARRA_GPU_OK);
    EXPECT_GPU_STATUS(clearra_gpu_confirmed_candidate_queue_from_result(
                          &gpu_result, &queue),
                      CLEARRA_GPU_OK);

    EXPECT_TRUE(queue.can_enter_cpu_buildup_queue);
    EXPECT_FALSE(queue.can_create_coverage_row);
    EXPECT_FALSE(queue.candidate_is_solution);
}
