#ifndef CLEARRA_HYBRID_AUTOTUNE_CONTRACT_H
#define CLEARRA_HYBRID_AUTOTUNE_CONTRACT_H

#include "hybrid_backpressure_contract.h"
typedef enum ClearraHybridMemoryPressureLevel {
    CLEARRA_HYBRID_MEMORY_PRESSURE_LOW = 0,
    CLEARRA_HYBRID_MEMORY_PRESSURE_MODERATE = 1,
    CLEARRA_HYBRID_MEMORY_PRESSURE_HIGH = 2
} ClearraHybridMemoryPressureLevel;typedef struct ClearraHybridAutotuneMetrics {
    uint32_t gpu_batches_submitted;
    uint32_t gpu_batches_completed;
    uint32_t gpu_readback_pending;
    uint32_t cpu_confirm_queue_depth;
    uint32_t cpu_buildup_queue_depth;
    uint32_t candidate_buffer_pressure;
    uint32_t coverage_row_buffer_pressure;
    uint32_t memory_ticket_live_count;
    uint32_t pending_release_queue_depth;
    uint32_t average_batch_latency_ms;
    uint32_t average_cpu_confirm_latency_ms;
} ClearraHybridAutotuneMetrics;typedef struct ClearraHybridAutotuneBudget {
    uint32_t min_batch_size;
    uint32_t max_batch_size;
    uint32_t max_readback_pending;
    uint32_t max_cpu_backlog;
    uint32_t max_memory_pressure;
    uint32_t max_coverage_buffer_pressure;
} ClearraHybridAutotuneBudget;typedef struct ClearraHybridMemoryPressureReport {
    ClearraHybridMemoryPressureLevel level;
    uint32_t memory_ticket_live_count;
    uint32_t pending_release_queue_depth;
    uint32_t pressure_score;
} ClearraHybridMemoryPressureReport;typedef struct ClearraHybridAutotuneDecision {
    uint32_t selected_batch_size;
    uint8_t throttle_gpu_submission;
    uint8_t prioritize_dedupe;
    uint8_t defer_low_priority_candidates;
    uint8_t reduce_trace_retention;
    uint8_t batch_scope_early_release;
    uint8_t throttle_coverage_row_emission;
    uint8_t count_only_mode_allowed;
    uint8_t partial_result_diagnostic_required;
    const char *truncation_reason;
    ClearraHybridThrottleReason throttle_reason;
    ClearraHybridMemoryPressureReport memory_pressure;
} ClearraHybridAutotuneDecision;ClearraHybridAutotuneBudget clearra_hybrid_autotune_budget_default(void);
uint32_t clearra_hybrid_batch_size_for(
    const ClearraHybridAutotuneBudget *budget,
    const ClearraHybridAutotuneMetrics *metrics);
ClearraHybridMemoryPressureReport clearra_hybrid_memory_pressure_report_for(
    const ClearraHybridAutotuneBudget *budget,
    const ClearraHybridAutotuneMetrics *metrics);
ClearraHybridAutotuneDecision clearra_hybrid_autotune_evaluate(
    const ClearraHybridAutotuneBudget *budget,
    const ClearraHybridAutotuneMetrics *metrics);
#endif
