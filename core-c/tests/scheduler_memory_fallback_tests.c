#include "scheduler_tests_support.h"
void fallback_reason_reported(void) {
    clr_packing_problem packing = scheduler_test_scheduler_packing_problem();
    ClearraGpuPackingBatchDescriptor batch = scheduler_test_scheduler_batch();
    static ClearraHybridSchedulerResult result;
    ClearraGpuDeviceRequest request = clearra_gpu_device_request_default();
    clearra_hybrid_scheduler_result_clear(&result);
    request.device_kind = (uint8_t)CLEARRA_GPU_BACKEND_NATIVE_COMPUTE;

    EXPECT_HYBRID_STATUS(clearra_hybrid_scheduler_run(
                             &packing, &batch, request, true, &result),
                         CLEARRA_HYBRID_OK);

    EXPECT_TRUE(result.metrics.fallback_used);
    EXPECT_U64(result.metrics.fallback_reason,
               CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE);
    EXPECT_U64(result.metrics.cpu_reference_candidate_count,
               result.metrics.hybrid_candidate_count);
}
void hybrid_scheduler_fallback_reports_reason(void) {
    fallback_reason_reported();
}
void memory_leak_report_clean(void) {
    clr_packing_problem packing = scheduler_test_scheduler_packing_problem();
    ClearraGpuPackingBatchDescriptor batch = scheduler_test_scheduler_batch();
    static ClearraHybridSchedulerResult result;
    clearra_hybrid_scheduler_result_clear(&result);

    EXPECT_HYBRID_STATUS(clearra_hybrid_scheduler_run_cpu_fallback(
                             &packing, &batch, &result),
                         CLEARRA_HYBRID_OK);

    EXPECT_TRUE(result.plan.memory_epoch_managed);
    EXPECT_TRUE(result.metrics.memory_epoch_end > result.metrics.memory_epoch_start);
    EXPECT_TRUE(result.metrics.memory_leak_report_clean);
    EXPECT_U64(result.leak_report.live_scopes, 0);
    EXPECT_U64(result.leak_report.live_allocations, 0);
    EXPECT_U64(result.leak_report.live_gpu_buffers, 0);
    EXPECT_U64(result.leak_report.pending_release_queue, 0);
}
void hybrid_scheduler_uses_scope_allocator_for_scratch_buffers(void) {
    ClrMemContext *context = NULL;
    ClrScope *scope = NULL;
    ClearraHybridScratch scratch;
    ClrMemLeakReport report;
    uint64_t epoch = 0;

    EXPECT_MEM_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_MEM_STATUS(clr_scope_create(context, CLR_SCOPE_WORKER, &scope), CLR_MEM_OK);
    EXPECT_HYBRID_STATUS(clearra_hybrid_scratch_create(scope, &scratch),
                         CLEARRA_HYBRID_OK);
    EXPECT_TRUE(scratch.owner_scope == scope);
    EXPECT_TRUE(scratch.cpu_table != NULL);
    EXPECT_TRUE(scratch.cpu_raw_candidates != NULL);
    EXPECT_TRUE(scratch.candidate_variants != NULL);
    EXPECT_TRUE(scratch.cpu_variants != NULL);
    EXPECT_TRUE(scratch.hybrid_variants != NULL);
    EXPECT_TRUE(scratch.cpu_coverage_rows != NULL);
    EXPECT_TRUE(scratch.hybrid_coverage_rows != NULL);
    EXPECT_MEM_STATUS(clr_mem_context_leak_report(context, &report), CLR_MEM_OK);
    EXPECT_U64(report.live_scopes, 1);
    EXPECT_U64(report.live_allocations, 7);

    EXPECT_MEM_STATUS(clr_release_queue_defer_scope(context, scope, 1), CLR_MEM_OK);
    EXPECT_MEM_STATUS(clr_epoch_advance(context, &epoch), CLR_MEM_OK);
    EXPECT_MEM_STATUS(clr_release_queue_drain(context, epoch), CLR_MEM_OK);
    EXPECT_MEM_STATUS(clr_mem_context_leak_report(context, &report), CLR_MEM_OK);
    EXPECT_U64(report.live_scopes, 0);
    EXPECT_U64(report.live_allocations, 0);
    EXPECT_U64(report.pending_release_queue, 0);
    EXPECT_MEM_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
}
void hybrid_scheduler_no_raw_malloc_in_hot_path(void) {
    hybrid_scheduler_uses_scope_allocator_for_scratch_buffers();
    memory_leak_report_clean();
}
void hybrid_scheduler_failure_has_clean_leak_report(void) {
    ClrMemContext *context = NULL;
    ClrScope *scope = NULL;
    ClearraHybridScratch scratch;
    ClrMemLeakReport report;

    EXPECT_MEM_STATUS(clr_mem_context_create(&context), CLR_MEM_OK);
    EXPECT_MEM_STATUS(clr_scope_create(context, CLR_SCOPE_WORKER, &scope), CLR_MEM_OK);
    EXPECT_HYBRID_STATUS(clearra_hybrid_scratch_create(scope, &scratch),
                         CLEARRA_HYBRID_OK);
    EXPECT_MEM_STATUS(clr_scope_abort(scope), CLR_MEM_OK);
    EXPECT_MEM_STATUS(clr_mem_context_leak_report(context, &report), CLR_MEM_OK);
    EXPECT_U64(report.live_scopes, 0);
    EXPECT_U64(report.live_allocations, 0);
    EXPECT_U64(report.aborted_scopes, 1);
    EXPECT_MEM_STATUS(clr_mem_context_release(&context), CLR_MEM_OK);
}
void no_fallback_reports_unavailable_without_cpu_work(void) {
    clr_packing_problem packing = scheduler_test_scheduler_packing_problem();
    ClearraGpuPackingBatchDescriptor batch = scheduler_test_scheduler_batch();
    static ClearraHybridSchedulerResult result;
    ClearraGpuDeviceRequest request = clearra_gpu_device_request_default();
    clearra_hybrid_scheduler_result_clear(&result);
    request.device_kind = (uint8_t)CLEARRA_GPU_BACKEND_NATIVE_COMPUTE;

    EXPECT_HYBRID_STATUS(clearra_hybrid_scheduler_run(
                             &packing, &batch, request, false, &result),
                         CLEARRA_HYBRID_GPU_UNAVAILABLE);

    EXPECT_FALSE(result.metrics.fallback_used);
    EXPECT_U64(result.metrics.fallback_reason,
               CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE);
}
