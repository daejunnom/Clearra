#include "gpu_test_support.h"
void no_backend_fallback_prevents_silent_cpu_fallback(void) {
    ClearraGpuPackingBatchDescriptor batch = standard_batch();
    static ClearraGpuPackingResult result;
    ClearraGpuDeviceRequest request = clearra_gpu_device_request_default();
    clearra_gpu_packing_result_clear(&result);
    request.device_kind = (uint8_t)CLEARRA_GPU_BACKEND_NATIVE_COMPUTE;

    EXPECT_GPU_STATUS(clearra_gpu_packing_backend_run(&batch, request, false, &result),
                      CLEARRA_GPU_UNAVAILABLE);
    EXPECT_FALSE(result.used_cpu_fallback);
    EXPECT_U64(result.raw_candidate_count, 0);
    EXPECT_U64(result.unavailable_reason, CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE);
}void gpu_backend_fallback_allowed_uses_cpu(void) {
    ClearraGpuPackingBatchDescriptor batch = standard_batch();
    static ClearraGpuPackingResult result;
    ClearraGpuDeviceRequest request = clearra_gpu_device_request_default();
    clearra_gpu_packing_result_clear(&result);
    request.device_kind = (uint8_t)CLEARRA_GPU_BACKEND_NATIVE_COMPUTE;

    EXPECT_GPU_STATUS(clearra_gpu_packing_backend_run(&batch, request, true, &result),
                      CLEARRA_GPU_OK);
    EXPECT_TRUE(result.used_cpu_fallback);
    EXPECT_U64(result.unavailable_reason, CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE);
    EXPECT_TRUE(result.raw_candidate_count > 0);
    EXPECT_FALSE(result.candidate_is_solution);
}void gpu_backend_no_fallback_returns_error(void) {
    ClearraGpuPackingBatchDescriptor batch = standard_batch();
    static ClearraGpuPackingResult result;
    ClearraGpuDeviceRequest request = clearra_gpu_device_request_default();
    clearra_gpu_packing_result_clear(&result);
    request.device_kind = (uint8_t)CLEARRA_GPU_BACKEND_NATIVE_COMPUTE;

    EXPECT_GPU_STATUS(clearra_gpu_packing_backend_run(&batch, request, false, &result),
                      CLEARRA_GPU_UNAVAILABLE);
    EXPECT_FALSE(result.used_cpu_fallback);
    EXPECT_U64(result.unavailable_reason, CLEARRA_GPU_UNAVAILABLE_KERNEL_UNAVAILABLE);
}void gpu_product_backend_records_pruning_evidence_and_matches_cpu(void) {
    ClearraGpuPackingBatchDescriptor batch = standard_batch();
    ClearraGpuDeviceRequest request = clearra_gpu_device_request_default();
    static ClearraGpuPackingResult result;
    static ClearraPackingCandidateBuffer cpu_reference;
    clr_packing_problem packing_problem;
    clr_static_prune_context prune_context;
    clearra_gpu_packing_result_clear(&result);
    request.device_kind = (uint8_t)CLEARRA_GPU_BACKEND_NATIVE_COMPUTE;

    EXPECT_GPU_STATUS(
        clearra_gpu_packing_backend_run(&batch, request, true, &result),
        CLEARRA_GPU_OK);
    cpu_reference_for_gpu_batch(&batch, &cpu_reference);
    EXPECT_GPU_STATUS(clearra_gpu_batch_descriptor_to_packing_problem(
                          &batch, &packing_problem),
                      CLEARRA_GPU_OK);
    EXPECT_PACKING_STATUS(clearra_packing_prune_context_from_problem(
                              &packing_problem, &prune_context),
                          CLEARRA_PACKING_OK);

    EXPECT_TRUE(result.cpu_reference_matched);
    EXPECT_TRUE(result.used_cpu_fallback);
    expect_candidate_buffers_match_canonical(
        &result.raw_candidates, &cpu_reference);
    EXPECT_TRUE(result.pruning_ledger.count > 0u);
    for (uint16_t index = 0u; index < result.pruning_ledger.count; ++index) {
        EXPECT_U64(result.pruning_ledger.entries[index].batch_id,
                   prune_context.batch_id);
        EXPECT_U64(
            result.pruning_ledger.entries[index].prune_reason,
            CLR_PRUNE_PLACEMENT_COLLISION);
        EXPECT_U64(
            result.pruning_ledger.entries[index].proof_level,
            CLR_PRUNE_PROOF_GLOBAL_SAFE);
    }
}void cpu_reference_differs_for_o_only_vs_mixed_piece_batch(void) {
    ClearraGpuPackingBatchDescriptor o_only = standard_batch();
    ClearraGpuPackingBatchDescriptor mixed = mixed_piece_batch();
    static ClearraGpuPackingResult o_result;
    static ClearraGpuPackingResult mixed_result;
    clearra_gpu_packing_result_clear(&o_result);
    clearra_gpu_packing_result_clear(&mixed_result);

    EXPECT_GPU_STATUS(clearra_cpu_packing_reference_run(
                          &o_only, &o_result),
                      CLEARRA_GPU_OK);
    EXPECT_GPU_STATUS(clearra_cpu_packing_reference_run(
                          &mixed, &mixed_result),
                      CLEARRA_GPU_OK);

    EXPECT_TRUE(o_result.raw_candidate_count > 0);
    EXPECT_TRUE(mixed_result.raw_candidate_count > 0);
    EXPECT_FALSE(o_result.gpu_candidate_hash == mixed_result.gpu_candidate_hash);
}void gpu_worker_unavailable_result_is_not_exact(void) {
    ClearraGpuPackingBatchDescriptor batch = standard_batch();
    ClearraGpuWorkerRequest request = {
        .request_id = 7,
        .batch = batch,
        .memory_ticket_id = 11,
        .fence_epoch = 3,
        .scope_epoch = 3,
        .byte_budget = 4096,
        .cpu_confirm_required = 1u,
    };
    static ClearraGpuWorkerResult result;

    EXPECT_GPU_STATUS((ClearraGpuStatus)clearra_gpu_worker_run(
                          &request, &result),
                      (ClearraGpuStatus)CLEARRA_GPU_WORKER_UNAVAILABLE);
    EXPECT_U64(result.request_id, 7);
    EXPECT_U64(result.memory_ticket_id, 11);
    EXPECT_U64(result.fence_epoch, 3);
    EXPECT_U64(result.scope_epoch, 3);
    EXPECT_U64(result.byte_budget, 4096);
    EXPECT_U64(result.trust_state, CLEARRA_GPU_WORKER_TRUST_UNAVAILABLE);
    EXPECT_FALSE(result.can_source_exact_probability);
    EXPECT_TRUE(result.cpu_confirm_required);
    EXPECT_U64(result.candidate_count, 0);
}void gpu_worker_unconfirmed_result_cannot_source_exact_probability(void) {
    EXPECT_FALSE(clearra_gpu_worker_trust_can_source_exact_probability(
        CLEARRA_GPU_WORKER_TRUST_GPU_COMPUTED_UNCONFIRMED));
    EXPECT_TRUE(clearra_gpu_worker_trust_can_source_exact_probability(
        CLEARRA_GPU_WORKER_TRUST_GPU_COMPUTED_CPU_CONFIRMED));
    EXPECT_TRUE(clearra_gpu_worker_trust_can_source_exact_probability(
        CLEARRA_GPU_WORKER_TRUST_DETERMINISTIC_REFERENCE_MATCHED));
    EXPECT_FALSE(clearra_gpu_worker_trust_can_source_exact_probability(
        CLEARRA_GPU_WORKER_TRUST_FALLBACK_USED));
    EXPECT_FALSE(clearra_gpu_worker_trust_can_source_exact_probability(
        CLEARRA_GPU_WORKER_TRUST_UNAVAILABLE));
    EXPECT_FALSE(clearra_gpu_worker_trust_can_source_exact_probability(
        CLEARRA_GPU_WORKER_TRUST_GPU_COMPUTED_MISMATCH));
}void gpu_worker_scheduler_bridge_uses_memory_ticket_and_fence(void) {
    ClrMemContext *context = NULL;
    ClrScope *scope = NULL;
    ClearraGpuPackingBatchDescriptor batch = standard_batch();
    static ClearraGpuWorkerResult result;
    ClrMemLeakReport report;

    if (clr_mem_context_create(&context) != CLR_MEM_OK ||
        clr_scope_create(context, CLR_SCOPE_GPU_TRANSFER, &scope) !=
            CLR_MEM_OK) {
        fprintf(stderr, "failed to create GPU worker memory scope\n");
        exit(1);
    }

    EXPECT_GPU_STATUS((ClearraGpuStatus)clearra_gpu_worker_scheduler_bridge_run(
                          context, scope, &batch, &result),
                      (ClearraGpuStatus)CLEARRA_GPU_WORKER_UNAVAILABLE);
    EXPECT_TRUE(result.memory_ticket_id != 0);
    EXPECT_TRUE(result.fence_epoch != 0);
    EXPECT_TRUE(result.scope_epoch != 0);
    EXPECT_TRUE(result.byte_budget >= sizeof(ClearraGpuPackingBatchDescriptor));
    EXPECT_FALSE(result.can_source_exact_probability);
    if (clr_scope_release(scope) != CLR_MEM_OK ||
        clr_mem_context_leak_report(context, &report) != CLR_MEM_OK) {
        fprintf(stderr, "failed to release GPU worker memory scope\n");
        exit(1);
    }
    EXPECT_U64(report.live_gpu_buffers, 0);
    EXPECT_U64(report.pending_gpu_buffer_releases, 0);
    if (clr_mem_context_release(&context) != CLR_MEM_OK) {
        fprintf(stderr, "failed to release GPU worker memory context\n");
        exit(1);
    }
}
