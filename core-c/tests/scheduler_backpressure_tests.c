#include "scheduler_tests_support.h"
void hybrid_cpu_fallback_reports_no_gpu_throttle(void) {
    clr_packing_problem packing = scheduler_test_scheduler_packing_problem();
    ClearraGpuPackingBatchDescriptor batch = scheduler_test_scheduler_batch();
    static ClearraHybridSchedulerResult result;
    clearra_hybrid_scheduler_result_clear(&result);

    EXPECT_HYBRID_STATUS(clearra_hybrid_scheduler_run_cpu_fallback(
                             &packing, &batch, &result),
                         CLEARRA_HYBRID_OK);

    EXPECT_U64(result.backpressure.gpu_queue_depth, 0);
    EXPECT_U64(result.backpressure.readback_pending_batches, 0);
    EXPECT_TRUE(result.metrics.fallback_used);
}void hybrid_scheduler_reports_cpu_fallback_backpressure_contract(void) {
    clr_packing_problem packing = scheduler_test_scheduler_packing_problem();
    ClearraGpuPackingBatchDescriptor batch = scheduler_test_scheduler_batch();
    static ClearraHybridSchedulerResult result;
    clearra_hybrid_scheduler_result_clear(&result);

    EXPECT_HYBRID_STATUS(clearra_hybrid_scheduler_run_cpu_fallback(
                             &packing, &batch, &result),
                         CLEARRA_HYBRID_OK);

    EXPECT_TRUE(result.backpressure.candidate_queue_len > 0u);
    EXPECT_U64(result.backpressure.candidate_queue_capacity,
               result.plan.large_batch_threshold);
    EXPECT_U64(result.backpressure.cpu_worker_backlog,
               result.backpressure.cpu_worker_queue_depth);
    EXPECT_U64(result.backpressure.gpu_readback_backlog,
               result.backpressure.readback_pending_batches);
    EXPECT_U64(result.backpressure.gpu_batch_in_flight, 0u);
    EXPECT_TRUE(result.backpressure.backpressure_active);
    EXPECT_U64(result.backpressure.memory_pressure_level,
               result.metrics.memory_pressure_level);
}void hybrid_scheduler_cpu_fallback_does_not_submit_gpu_worker(void) {
    clr_packing_problem packing = scheduler_test_scheduler_packing_problem();
    ClearraGpuPackingBatchDescriptor batch = scheduler_test_scheduler_batch();
    static ClearraHybridSchedulerResult result;
    clearra_hybrid_scheduler_result_clear(&result);

    EXPECT_HYBRID_STATUS(clearra_hybrid_scheduler_run_cpu_fallback(
                             &packing, &batch, &result),
                         CLEARRA_HYBRID_OK);

    EXPECT_TRUE(result.metrics.cpu_preprocessor_batch_descriptor_created);
    EXPECT_FALSE(result.metrics.gpu_worker_request_submitted);
    EXPECT_U64(result.metrics.gpu_worker_request_id, 0);
    EXPECT_U64(result.metrics.gpu_worker_memory_ticket_id, 0);
    EXPECT_U64(result.metrics.gpu_worker_fence_epoch, 0);
    EXPECT_U64(result.metrics.gpu_worker_trust_state,
               CLEARRA_GPU_WORKER_TRUST_NOT_USED);
    EXPECT_TRUE(result.metrics.fallback_used);
    EXPECT_TRUE(result.metrics.cpu_exact_confirm_queue_received);
    EXPECT_TRUE(result.metrics.cpu_exact_confirm_queue_depth > 0);
}
void hybrid_cpu_fallback_reports_zero_gpu_queue_depth(void) {
    clr_packing_problem packing = scheduler_test_scheduler_packing_problem();
    ClearraGpuPackingBatchDescriptor batch = scheduler_test_scheduler_batch();
    static ClearraHybridSchedulerResult result;
    clearra_hybrid_scheduler_result_clear(&result);

    EXPECT_HYBRID_STATUS(clearra_hybrid_scheduler_run_cpu_fallback(
                             &packing, &batch, &result),
                         CLEARRA_HYBRID_OK);

    EXPECT_U64(result.metrics.gpu_queue_depth, 0);
    EXPECT_U64(result.backpressure.gpu_queue_depth,
               result.metrics.gpu_queue_depth);
}
void hybrid_cpu_fallback_reports_zero_readback_pending(void) {
    clr_packing_problem packing = scheduler_test_scheduler_packing_problem();
    ClearraGpuPackingBatchDescriptor batch = scheduler_test_scheduler_batch();
    static ClearraHybridSchedulerResult result;
    clearra_hybrid_scheduler_result_clear(&result);

    EXPECT_HYBRID_STATUS(clearra_hybrid_scheduler_run_cpu_fallback(
                             &packing, &batch, &result),
                         CLEARRA_HYBRID_OK);

    EXPECT_U64(result.metrics.readback_pending_batches, 0);
    EXPECT_U64(result.backpressure.readback_pending_batches,
               result.metrics.readback_pending_batches);
    EXPECT_U64(result.backpressure.throttle_reason,
               CLEARRA_HYBRID_THROTTLE_CPU_WORKER_QUEUE_DEPTH);
}
void hybrid_scheduler_throttles_when_cpu_buildup_backlog_high(void) {
    ClearraHybridBatchPlan plan = clearra_hybrid_batch_plan_for(0);
    ClearraHybridBackendMetrics metrics = {0};
    ClearraHybridBackpressureReport report;
    plan.cpu_worker_count = 1u;
    plan.large_batch_threshold = 64u;
    metrics.cpu_buildup_backlog = 4u;

    report = clearra_hybrid_backpressure_report_for(&plan, &metrics);

    EXPECT_U64(report.cpu_worker_queue_depth, 4);
    EXPECT_U64(report.cpu_worker_backlog, 4);
    EXPECT_TRUE(report.backpressure_active);
    EXPECT_TRUE(report.deferred_batch_count > 0u);
    EXPECT_U64(report.throttle_reason,
               CLEARRA_HYBRID_THROTTLE_CPU_WORKER_QUEUE_DEPTH);
}
void hybrid_scheduler_throttles_when_coverage_buffer_pressure_high(void) {
    ClearraHybridBatchPlan plan = clearra_hybrid_batch_plan_for(0);
    ClearraHybridBackendMetrics metrics = {0};
    ClearraHybridBackpressureReport report;
    plan.cpu_worker_count = 1u;
    plan.large_batch_threshold = 64u;
    metrics.coverage_row_buffer_pressure = 7u;

    report = clearra_hybrid_backpressure_report_for(&plan, &metrics);

    EXPECT_U64(report.coverage_row_buffer_pressure, 7);
    EXPECT_TRUE(report.backpressure_active);
    EXPECT_TRUE(report.truncated_batch_count > 0u);
    EXPECT_U64(report.throttle_reason,
               CLEARRA_HYBRID_THROTTLE_COVERAGE_ROW_BUFFER_PRESSURE);
}
void hybrid_gpu_queue_tracks_submitted_completed_and_latency(void) {
    ClearraHybridGpuQueueStats queue;
    ClearraHybridBackendMetrics backend_metrics = {0};
    ClearraHybridAutotuneMetrics autotune_metrics = {0};

    clearra_hybrid_gpu_queue_init(&queue);
    clearra_hybrid_gpu_queue_submit(&queue, 2u);
    clearra_hybrid_gpu_queue_complete(&queue, 1u, 7u);
    clearra_hybrid_gpu_queue_complete(&queue, 1u, 5u);
    clearra_hybrid_gpu_queue_apply_metrics(
        &queue, &backend_metrics, &autotune_metrics);

    EXPECT_U64(backend_metrics.gpu_batches_submitted, 2);
    EXPECT_U64(backend_metrics.gpu_batches_completed, 2);
    EXPECT_U64(backend_metrics.gpu_queue_depth, 2);
    EXPECT_U64(backend_metrics.average_batch_latency_ms, 6);
    EXPECT_U64(autotune_metrics.average_batch_latency_ms, 6);
}
void hybrid_readback_queue_tracks_pending_and_candidate_pressure(void) {
    ClearraHybridReadbackQueueStats queue;
    ClearraHybridBackendMetrics backend_metrics = {0};
    ClearraHybridAutotuneMetrics autotune_metrics = {0};

    clearra_hybrid_readback_queue_init(&queue);
    clearra_hybrid_readback_queue_enqueue(&queue, 1u, 9u);
    clearra_hybrid_readback_queue_enqueue(&queue, 1u, 13u);
    clearra_hybrid_readback_queue_complete(&queue, 2u);
    clearra_hybrid_readback_queue_apply_metrics(
        &queue, &backend_metrics, &autotune_metrics);

    EXPECT_U64(backend_metrics.readback_pending_batches, 2);
    EXPECT_U64(backend_metrics.gpu_readback_pending, 2);
    EXPECT_U64(backend_metrics.candidate_buffer_pressure, 13);
    EXPECT_U64(autotune_metrics.candidate_buffer_pressure, 13);
}
void hybrid_cpu_confirm_queue_tracks_confirm_and_buildup_depth(void) {
    ClearraHybridCpuConfirmQueueStats queue;
    ClearraHybridBackendMetrics backend_metrics = {0};
    ClearraHybridAutotuneMetrics autotune_metrics = {0};

    clearra_hybrid_cpu_confirm_queue_init(&queue);
    clearra_hybrid_cpu_confirm_queue_enqueue(&queue, 4u);
    clearra_hybrid_cpu_confirm_queue_complete(&queue, 4u, 6u, 12u);
    clearra_hybrid_cpu_confirm_queue_apply_metrics(
        &queue, &backend_metrics, &autotune_metrics);

    EXPECT_U64(backend_metrics.cpu_confirm_queue_depth, 4);
    EXPECT_U64(backend_metrics.cpu_exact_confirm_queue_depth, 4);
    EXPECT_U64(backend_metrics.cpu_buildup_queue_depth, 6);
    EXPECT_U64(backend_metrics.cpu_buildup_backlog, 6);
    EXPECT_U64(backend_metrics.average_cpu_confirm_latency_ms, 3);
    EXPECT_U64(autotune_metrics.average_cpu_confirm_latency_ms, 3);
}
void hybrid_cpu_fallback_metrics_exclude_gpu_queue_stats(void) {
    clr_packing_problem packing = scheduler_test_scheduler_packing_problem();
    ClearraGpuPackingBatchDescriptor batch = scheduler_test_scheduler_batch();
    static ClearraHybridSchedulerResult result;
    clearra_hybrid_scheduler_result_clear(&result);

    EXPECT_HYBRID_STATUS(clearra_hybrid_scheduler_run_cpu_fallback(
                             &packing, &batch, &result),
                         CLEARRA_HYBRID_OK);

    EXPECT_U64(result.metrics.gpu_batches_submitted, 0);
    EXPECT_U64(result.metrics.gpu_batches_completed, 0);
    EXPECT_U64(result.metrics.gpu_readback_pending, 0);
    EXPECT_U64(result.metrics.cpu_confirm_queue_depth,
               result.metrics.cpu_exact_confirm_queue_depth);
    EXPECT_U64(result.metrics.cpu_buildup_queue_depth,
               result.metrics.cpu_buildup_backlog);
    EXPECT_U64(result.metrics.candidate_buffer_pressure, 0);
    EXPECT_U64(result.metrics.average_batch_latency_ms, 0);
    EXPECT_TRUE(result.metrics.cpu_exact_confirm_queue_received);
}
