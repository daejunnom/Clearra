#ifndef CLEARRA_HYBRID_BACKEND_METRICS_H
#define CLEARRA_HYBRID_BACKEND_METRICS_H

#include "../../src/gpu/gpu_backend.h"
#include "../../include/clr_gpu_worker.h"

#include <stdint.h>

typedef struct ClearraHybridBackendMetrics {
    uint8_t cpu_preprocessor_batch_descriptor_created;
    uint8_t gpu_worker_request_submitted;
    uint64_t gpu_worker_request_id;
    uint64_t gpu_worker_memory_ticket_id;
    uint64_t gpu_worker_fence_epoch;
    ClearraGpuWorkerTrustState gpu_worker_trust_state;
    uint16_t cpu_reference_candidate_count;
    uint16_t hybrid_candidate_count;
    uint16_t cpu_reference_build_variant_count;
    uint16_t hybrid_build_variant_count;
    uint16_t gpu_queue_depth;
    uint16_t readback_pending_batches;
    uint16_t cpu_buildup_backlog;
    uint16_t cpu_exact_confirm_queue_depth;
    uint16_t coverage_row_buffer_pressure;
    uint32_t gpu_batches_submitted;
    uint32_t gpu_batches_completed;
    uint32_t gpu_readback_pending;
    uint32_t cpu_confirm_queue_depth;
    uint32_t cpu_buildup_queue_depth;
    uint32_t candidate_buffer_pressure;
    uint32_t memory_ticket_live_count;
    uint32_t pending_release_queue_depth;
    uint32_t average_batch_latency_ms;
    uint32_t average_cpu_confirm_latency_ms;
    uint8_t cpu_exact_confirm_queue_received;
    uint8_t memory_pressure_level;
    uint16_t batch_buffers_reused;
    uint16_t work_steal_count;
    uint16_t gpu_readback_overlap_steps;
    uint64_t memory_epoch_start;
    uint64_t memory_epoch_end;
    ClearraGpuUnavailableReason fallback_reason;
    uint8_t fallback_used;
    uint8_t backend_metrics_reported;
    uint8_t memory_leak_report_clean;
    uint8_t gpu_only_packing_cpu_buildup_matches_cpu_reference;
    uint8_t gpu_assisted_buildup_reached;
    uint8_t buildup_dispatch_mode;
    uint16_t cpu_reference_coverage_row_count;
    uint16_t hybrid_coverage_row_count;
    uint8_t coverage_rows_from_enumerate_variants;
    uint8_t verify_first_used_for_coverage;
    uint16_t failure_stage;
} ClearraHybridBackendMetrics;

#endif
