#include "scheduler_tests_support.h"
void cpu_only_result_equals_hybrid_result(void) {
    clr_packing_problem packing;
    ClearraGpuPackingBatchDescriptor batch;
    static ClearraHybridSchedulerResult result;
    scheduler_test_scheduler_packing_problem_into(&packing);
    scheduler_test_scheduler_batch_into(&batch);
    clearra_hybrid_scheduler_result_clear(&result);

    EXPECT_HYBRID_STATUS(clearra_hybrid_scheduler_run_cpu_fallback(
                             &packing, &batch, &result),
                         CLEARRA_HYBRID_OK);

    EXPECT_U64(result.metrics.cpu_reference_candidate_count,
               result.metrics.hybrid_candidate_count);
    EXPECT_U64(result.metrics.cpu_reference_build_variant_count,
               result.metrics.hybrid_build_variant_count);
}
void hybrid_cpu_fallback_returns_confirmed_candidates_for_product_path(void) {
    clr_packing_problem packing = scheduler_test_scheduler_packing_problem();
    ClearraGpuPackingBatchDescriptor batch = scheduler_test_scheduler_batch();
    static ClearraHybridSchedulerResult result;
    static ClearraPackingCandidateBuffer confirmed;
    static ClearraPackingCandidateBuffer cpu_raw;
    static ClearraCanonicalPackingTable cpu_table;
    clearra_hybrid_scheduler_result_clear(&result);

    EXPECT_HYBRID_STATUS(clearra_hybrid_scheduler_run_cpu_fallback_candidates(
                             &packing, &batch, &result, &confirmed),
                         CLEARRA_HYBRID_OK);
    EXPECT_PACKING_STATUS(clearra_packing_enumerator_cpu_generate_problem(
                              &packing, &cpu_raw),
                          CLEARRA_PACKING_OK);
    EXPECT_PACKING_STATUS(clearra_packing_host_reduce(&cpu_raw, &cpu_table),
                          CLEARRA_PACKING_OK);

    EXPECT_U64(confirmed.count, result.metrics.hybrid_candidate_count);
    EXPECT_TRUE(clearra_packing_candidate_buffer_exactly_matches(
        &cpu_table.candidates, &confirmed));
}
void gpu_only_packing_plus_cpu_buildup_matches_cpu_reference(void) {
    clr_packing_problem packing = scheduler_test_scheduler_packing_problem();
    ClearraGpuPackingBatchDescriptor batch = scheduler_test_scheduler_batch();
    static ClearraHybridSchedulerResult result;
    clearra_hybrid_scheduler_result_clear(&result);

    EXPECT_HYBRID_STATUS(clearra_hybrid_scheduler_run_cpu_fallback(
                             &packing, &batch, &result),
                         CLEARRA_HYBRID_OK);

    EXPECT_TRUE(result.metrics.gpu_only_packing_cpu_buildup_matches_cpu_reference);
    EXPECT_TRUE(result.plan.batch_buffer_reuse);
    EXPECT_TRUE(result.plan.cpu_small_irregular_buildup);
}
void gpu_confirmed_candidate_builds_variant_only_after_cpu_buildup(void) {
    clr_packing_problem packing = scheduler_test_scheduler_packing_problem();
    ClearraGpuPackingBatchDescriptor batch = scheduler_test_scheduler_batch();
    static ClearraHybridSchedulerResult result;
    clearra_hybrid_scheduler_result_clear(&result);

    EXPECT_HYBRID_STATUS(clearra_hybrid_scheduler_run_cpu_fallback(
                             &packing, &batch, &result),
                         CLEARRA_HYBRID_OK);

    EXPECT_TRUE(result.metrics.cpu_exact_confirm_queue_received);
    EXPECT_TRUE(result.metrics.cpu_exact_confirm_queue_depth > 0);
    EXPECT_TRUE(result.metrics.hybrid_candidate_count > 0);
    EXPECT_TRUE(result.metrics.hybrid_build_variant_count > 0);
    EXPECT_U64(result.metrics.coverage_row_buffer_pressure,
               result.metrics.hybrid_build_variant_count);
}
void coverage_row_created_only_after_buildup_acceptance(void) {
    clr_packing_problem packing = scheduler_test_scheduler_packing_problem();
    ClearraGpuPackingBatchDescriptor batch = scheduler_test_scheduler_batch();
    static ClearraHybridSchedulerResult result;
    clearra_hybrid_scheduler_result_clear(&result);

    EXPECT_HYBRID_STATUS(clearra_hybrid_scheduler_run_cpu_fallback(
                             &packing, &batch, &result),
                         CLEARRA_HYBRID_OK);

    EXPECT_TRUE(result.metrics.cpu_exact_confirm_queue_received);
    EXPECT_TRUE(result.metrics.cpu_exact_confirm_queue_depth > 0);
    EXPECT_TRUE(result.metrics.hybrid_candidate_count > 0);
    EXPECT_TRUE(result.metrics.hybrid_build_variant_count > 0);
    EXPECT_FALSE(result.metrics.coverage_row_buffer_pressure >
                 result.metrics.hybrid_build_variant_count);
    EXPECT_U64(result.metrics.coverage_row_buffer_pressure,
               result.metrics.hybrid_build_variant_count);
    EXPECT_TRUE(result.metrics.coverage_rows_from_enumerate_variants);
}
void gpu_assisted_opening_2l_reaches_buildup(void) {
    clr_packing_problem packing = scheduler_test_scheduler_packing_problem();
    ClearraGpuPackingBatchDescriptor batch = scheduler_test_scheduler_batch();
    static ClearraHybridSchedulerResult result;
    clearra_hybrid_scheduler_result_clear(&result);

    EXPECT_HYBRID_STATUS(clearra_hybrid_scheduler_run_cpu_fallback(
                             &packing, &batch, &result),
                         CLEARRA_HYBRID_OK);

    EXPECT_TRUE(result.metrics.gpu_assisted_buildup_reached);
    EXPECT_U64(result.metrics.buildup_dispatch_mode,
               CLEARRA_HYBRID_BUILDUP_ENUMERATE_VARIANTS);
    EXPECT_TRUE(result.metrics.hybrid_build_variant_count > 0);
}
void gpu_assisted_buildvariant_count_matches_cpu_reference(void) {
    clr_packing_problem packing = scheduler_test_scheduler_packing_problem();
    ClearraGpuPackingBatchDescriptor batch = scheduler_test_scheduler_batch();
    static ClearraHybridSchedulerResult result;
    clearra_hybrid_scheduler_result_clear(&result);

    EXPECT_HYBRID_STATUS(clearra_hybrid_scheduler_run_cpu_fallback(
                             &packing, &batch, &result),
                         CLEARRA_HYBRID_OK);

    EXPECT_U64(result.metrics.cpu_reference_build_variant_count,
               result.metrics.hybrid_build_variant_count);
    EXPECT_TRUE(result.metrics.hybrid_build_variant_count > 0);
}
void gpu_assisted_coverage_rows_match_cpu_reference(void) {
    clr_packing_problem packing = scheduler_test_scheduler_packing_problem();
    ClearraGpuPackingBatchDescriptor batch = scheduler_test_scheduler_batch();
    static ClearraHybridSchedulerResult result;
    clearra_hybrid_scheduler_result_clear(&result);

    EXPECT_HYBRID_STATUS(clearra_hybrid_scheduler_run_cpu_fallback(
                             &packing, &batch, &result),
                         CLEARRA_HYBRID_OK);

    EXPECT_U64(result.metrics.cpu_reference_coverage_row_count,
               result.metrics.hybrid_coverage_row_count);
    EXPECT_U64(result.metrics.hybrid_coverage_row_count,
               result.metrics.hybrid_build_variant_count);
    EXPECT_TRUE(result.metrics.coverage_rows_from_enumerate_variants);
    EXPECT_FALSE(result.metrics.verify_first_used_for_coverage);
}
void gpu_verify_first_not_used_for_coverage(void) {
    clr_packing_problem packing = scheduler_test_scheduler_packing_problem();
    ClearraGpuPackingBatchDescriptor batch = scheduler_test_scheduler_batch();
    static ClearraGpuPackingResult gpu_result;
    ClearraGpuConfirmedCandidateQueue queue;
    static clr_build_variant_buffer first_variants;
    static clr_build_variant_buffer candidate_scratch;
    ClearraHybridBuildVariantCollection collection;
    static clr_coverage_row_view rows[CLR_BUILDUP_MAX_VARIANTS];
    ClearraHybridCoverageRowBridgeReport report;
    clearra_gpu_packing_result_clear(&gpu_result);

    EXPECT_GPU_STATUS(clearra_cpu_packing_reference_run(
                          &batch, &gpu_result),
                      CLEARRA_GPU_OK);
    EXPECT_GPU_STATUS(clearra_gpu_confirmed_candidate_queue_from_result(
                          &gpu_result, &queue),
                      CLEARRA_GPU_OK);
    EXPECT_HYBRID_STATUS(
        clearra_hybrid_collect_build_variants_from_confirmed_queue(
            &packing,
            &queue,
            CLEARRA_HYBRID_BUILDUP_VERIFY_FIRST,
            &candidate_scratch,
            &first_variants,
            &collection),
        CLEARRA_HYBRID_OK);

    EXPECT_TRUE(collection.verify_first_used_for_coverage);
    EXPECT_HYBRID_STATUS(clearra_hybrid_coverage_rows_from_build_variants(
                             CLEARRA_HYBRID_BUILDUP_VERIFY_FIRST,
                             &first_variants,
                             batch.piece_source_id,
                             batch.pattern_universe_id,
                             batch.pattern_weight_model_id,
                             queue.count,
                             rows,
                             CLR_BUILDUP_MAX_VARIANTS,
                             &report),
                         CLEARRA_HYBRID_INVALID_ARGUMENT);
    EXPECT_TRUE(report.rejected_verify_first);
}void hybrid_collect_uses_piece_source_pattern_id_not_candidate_index(void) {
    clr_packing_problem packing = scheduler_test_scheduler_packing_problem();
    ClearraGpuPackingBatchDescriptor batch = scheduler_test_scheduler_batch();
    static ClearraGpuPackingResult gpu_result;
    ClearraGpuConfirmedCandidateQueue queue;
    static clr_build_variant_buffer variants;
    static clr_build_variant_buffer candidate_scratch;
    ClearraHybridBuildVariantCollection collection;
    clearra_gpu_packing_result_clear(&gpu_result);
    packing.piece_source_pattern_id = 3u;

    EXPECT_GPU_STATUS(clearra_cpu_packing_reference_run(
                          &batch, &gpu_result),
                      CLEARRA_GPU_OK);
    EXPECT_GPU_STATUS(clearra_gpu_confirmed_candidate_queue_from_result(
                          &gpu_result, &queue),
                      CLEARRA_GPU_OK);
    EXPECT_HYBRID_STATUS(
        clearra_hybrid_collect_build_variants_from_confirmed_queue(
            &packing,
            &queue,
            CLEARRA_HYBRID_BUILDUP_ENUMERATE_VARIANTS,
            &candidate_scratch,
            &variants,
            &collection),
        CLEARRA_HYBRID_OK);

    EXPECT_TRUE(variants.count > 0u);
    for (uint16_t index = 0; index < variants.count; index++) {
        EXPECT_U64(variants.variants[index].coverage_pattern_id, 3u);
    }
}void hybrid_collect_rejects_kick_evidence_count_over_limit(void) {
    static clr_build_variant_buffer buffer;
    clr_build_variant_view variant = {0};
    clr_kick_evidence_view evidence[1] = {0};
    clr_build_variant_buffer_clear(&buffer);

    variant.kick_evidence = evidence;
    variant.kick_evidence_count =
        CLR_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT + 1u;

    EXPECT_HYBRID_STATUS(
        clearra_hybrid_build_variant_buffer_append_checked(&buffer, &variant),
        CLEARRA_HYBRID_BUILDUP_ERROR);
    EXPECT_U64(buffer.count, 0);
}void backend_metrics_reported(void) {
    clr_packing_problem packing = scheduler_test_scheduler_packing_problem();
    ClearraGpuPackingBatchDescriptor batch = scheduler_test_scheduler_batch();
    static ClearraHybridSchedulerResult result;
    clearra_hybrid_scheduler_result_clear(&result);

    EXPECT_HYBRID_STATUS(clearra_hybrid_scheduler_run_cpu_fallback(
                             &packing, &batch, &result),
                         CLEARRA_HYBRID_OK);

    EXPECT_TRUE(result.metrics.backend_metrics_reported);
    EXPECT_U64(result.metrics.gpu_readback_overlap_steps, 0);
    EXPECT_TRUE(result.metrics.fallback_used);
    EXPECT_TRUE(result.metrics.batch_buffers_reused >= 3);
    EXPECT_TRUE(result.metrics.work_steal_count > 0);
}void hybrid_result_reports_backend_metrics(void) {
    clr_packing_problem packing = scheduler_test_scheduler_packing_problem();
    ClearraGpuPackingBatchDescriptor batch = scheduler_test_scheduler_batch();
    static ClearraHybridSchedulerResult result;
    clearra_hybrid_scheduler_result_clear(&result);

    EXPECT_HYBRID_STATUS(clearra_hybrid_scheduler_run_cpu_fallback(
                             &packing, &batch, &result),
                         CLEARRA_HYBRID_OK);

    EXPECT_TRUE(result.metrics.backend_metrics_reported);
    EXPECT_U64(result.metrics.gpu_batches_submitted, 0u);
    EXPECT_U64(result.metrics.gpu_batches_completed, 0u);
    EXPECT_TRUE(result.metrics.fallback_used);
    EXPECT_U64(result.metrics.fallback_reason,
               CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE);
    EXPECT_TRUE(result.metrics.cpu_confirm_queue_depth > 0u);
    EXPECT_TRUE(result.metrics.cpu_buildup_queue_depth > 0u);
    EXPECT_TRUE(result.backpressure.candidate_queue_len > 0u);
}
