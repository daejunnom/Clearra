#ifndef CLEARRA_GPU_BACKEND_H
#define CLEARRA_GPU_BACKEND_H

#include "clr_gpu.h"
#include "../packing/packing_problem.h"

#include <stdbool.h>
#include <stdint.h>

/* Product GPU packing ABI: ClearraGpuPackingBatchDescriptor. */
#if defined(CLEARRA_CORE_TEST)
typedef struct ClearraGpuDevice {
    uint8_t available;
    ClearraGpuBackendKind backend_kind;
    uint8_t device_index;
    ClearraGpuUnavailableReason unavailable_reason;
} ClearraGpuDevice;typedef struct ClearraShapeUnionMask {
    uint64_t value;
} ClearraShapeUnionMask;typedef struct ClearraGpuPackingResult {
    ClearraGpuStatus status;
    ClearraGpuUnavailableReason unavailable_reason;
    uint8_t result_complete;
    uint16_t truncation_reason;
    uint8_t used_cpu_fallback;
    uint8_t candidate_is_solution;
    uint8_t hash_exact_confirmed;
    uint8_t deterministic_result;
    uint8_t larger_batch_planner_enabled;
    uint16_t planned_batch_count;
    uint16_t batch_candidate_capacity;
    uint8_t dominance_prefilter_applied;
    uint16_t dominance_prefilter_removed_count;
    uint8_t shape_union_mask_applied;
    ClearraShapeUnionMask gpu_shape_union_mask;
    uint64_t gpu_candidate_hash;
    uint64_t cpu_reference_hash;
    uint8_t readback_compressed;
    uint16_t readback_uncompressed_count;
    uint16_t readback_compressed_count;
    uint8_t cpu_exact_confirmed;
    uint8_t cpu_exact_confirm_optimized;
    uint8_t cpu_reference_matched;
    uint16_t raw_candidate_count;
    uint16_t canonical_candidate_count;
    clr_pruning_proof_ledger pruning_ledger;
    ClearraPackingCandidateBuffer raw_candidates;
    ClearraCanonicalPackingTable canonical_candidates;
} ClearraGpuPackingResult;typedef struct ClearraGpuConfirmedCandidateQueue {
    const ClearraCanonicalPackingTable *table;
    uint16_t count;
    uint8_t cpu_exact_confirmed;
    uint8_t candidate_is_solution;
    uint8_t can_enter_cpu_buildup_queue;
    uint8_t can_create_coverage_row;
} ClearraGpuConfirmedCandidateQueue;
#endif

#if defined(CLEARRA_CORE_TEST)
const char *clearra_gpu_unavailable_reason_label(ClearraGpuUnavailableReason reason);
const char *clearra_gpu_backend_kind_label(ClearraGpuBackendKind backend_kind);
const char *clearra_gpu_backend_kind_capability_label(
    ClearraGpuBackendKind backend_kind,
    uint8_t available);
ClearraGpuDeviceRequest clearra_gpu_device_request_default(void);
ClearraGpuStatus clearra_gpu_backend_capability(
    ClearraGpuBackendKind backend_kind,
    ClearraGpuBackendCapability *out_capability);
ClearraGpuStatus clearra_gpu_backend_select(
    ClearraGpuDeviceRequest request,
    ClearraGpuBackendCapability *out_capability);
ClearraGpuStatus clearra_gpu_backend_reject_user_provided_shader_path(
    const char *shader_path);
ClearraGpuStatus clearra_gpu_device_resolve(
    ClearraGpuDeviceRequest request,
    ClearraGpuDevice *out_device);
void clearra_gpu_packing_result_clear(ClearraGpuPackingResult *result);
#endif

ClearraGpuStatus clearra_gpu_batch_descriptor_init(
    ClearraBoard64Layout layout,
    uint64_t initial_board,
    uint8_t active_packing_rows,
    const uint8_t *pieces,
    uint8_t piece_count,
    ClearraGpuPackingBatchDescriptor *out_batch);
ClearraGpuStatus clearra_gpu_batch_descriptor_validate(
    const ClearraGpuPackingBatchDescriptor *batch);
ClearraGpuStatus clearra_gpu_batch_descriptor_layout(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraBoard64Layout *out_layout);
ClearraGpuStatus clearra_gpu_batch_descriptor_piece_multiset_window(
    const ClearraGpuPackingBatchDescriptor *batch,
    clr_gpu_piece_multiset_window *out_window);
ClearraGpuStatus clearra_gpu_batch_descriptor_product_source_of_truth(
    const ClearraGpuPackingBatchDescriptor *batch,
    uint64_t *out_piece_source_id,
    uint64_t *out_pattern_universe_id,
    uint64_t *out_pattern_weight_model_id,
    clr_gpu_piece_multiset_window *out_window);
ClearraGpuStatus clearra_gpu_batch_descriptor_to_packing_problem(
    const ClearraGpuPackingBatchDescriptor *batch,
    clr_packing_problem *out_problem);

#if defined(CLEARRA_CORE_TEST)
ClearraGpuStatus clearra_gpu_backend_dispatch_candidates(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraGpuBackendKind backend_kind,
    ClearraPackingCandidateBuffer *out_buffer);
ClearraGpuStatus clearra_gpu_backend_dispatch_candidates_with_pruning_ledger(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraGpuBackendKind backend_kind,
    ClearraPackingCandidateBuffer *out_buffer,
    clr_pruning_proof_ledger *out_pruning_ledger);
ClearraGpuStatus clearra_gpu_unavailable_backend_dispatch(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraGpuBackendKind backend_kind,
    ClearraPackingCandidateBuffer *out_buffer);
ClearraGpuStatus clearra_cpu_packing_reference_generate(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraPackingCandidateBuffer *out_buffer);
ClearraGpuStatus clearra_cpu_packing_reference_generate_with_resource_report(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraPackingCandidateBuffer *out_buffer,
    clr_resource_report *out_resource_report);
ClearraGpuStatus
clearra_cpu_packing_reference_generate_with_resource_report_and_pruning_ledger(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraPackingCandidateBuffer *out_buffer,
    clr_resource_report *out_resource_report,
    clr_pruning_proof_ledger *out_pruning_ledger);
ClearraGpuStatus clearra_gpu_host_confirm_candidates(
    const ClearraPackingCandidateBuffer *buffer,
    uint8_t *out_confirmed);
ClearraGpuStatus clearra_gpu_shape_union_mask(
    const ClearraPackingCandidateBuffer *buffer,
    ClearraShapeUnionMask *out_shape_union_mask);
ClearraGpuStatus clearra_gpu_candidate_hash(
    const ClearraPackingCandidateBuffer *buffer,
    uint64_t *out_hash);
ClearraGpuStatus clearra_gpu_dominance_prefilter_apply(
    ClearraPackingCandidateBuffer *buffer,
    uint16_t *out_removed_count);
ClearraGpuStatus clearra_gpu_readback_compress_candidates(
    const ClearraPackingCandidateBuffer *buffer,
    ClearraCanonicalPackingTable *out_table,
    uint16_t *out_compressed_count);
ClearraGpuStatus clearra_gpu_readback_reduce_result(
    ClearraGpuPackingResult *result);
ClearraGpuStatus clearra_gpu_cpu_exact_confirm_reference(
    const ClearraGpuPackingBatchDescriptor *batch,
    const ClearraGpuPackingResult *result,
    uint8_t *out_matched,
    uint64_t *out_cpu_reference_hash);
void clearra_gpu_confirmed_candidate_queue_clear(
    ClearraGpuConfirmedCandidateQueue *queue);
uint8_t clearra_gpu_raw_candidate_buffer_can_enter_buildup_queue(
    const ClearraPackingCandidateBuffer *raw_buffer);
uint8_t clearra_gpu_raw_candidate_buffer_can_create_coverage_row(
    const ClearraPackingCandidateBuffer *raw_buffer);
ClearraGpuStatus clearra_gpu_confirmed_candidate_queue_from_result(
    const ClearraGpuPackingResult *result,
    ClearraGpuConfirmedCandidateQueue *out_queue);
ClearraGpuStatus clearra_gpu_confirmed_candidate_queue_candidate_at(
    const ClearraGpuConfirmedCandidateQueue *queue,
    uint16_t index,
    ClearraPackingCandidateView *out_candidate);
bool clearra_gpu_fallback_allowed(bool allow_backend_fallback);
ClearraGpuStatus clearra_gpu_fallback_to_cpu_packing(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraGpuPackingResult *out_result);
ClearraGpuStatus clearra_gpu_packing_backend_run(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraGpuDeviceRequest request,
    bool allow_backend_fallback,
    ClearraGpuPackingResult *out_result);
ClearraGpuStatus clearra_cpu_packing_reference_run(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraGpuPackingResult *out_result);
#endif
#endif
