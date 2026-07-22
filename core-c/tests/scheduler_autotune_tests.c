#include "scheduler_tests_support.h"
void autotune_reduces_batch_size_when_cpu_backlog_high(void) {
    ClearraHybridAutotuneBudget budget =
        clearra_hybrid_autotune_budget_default();
    ClearraHybridAutotuneMetrics metrics = {0};
    ClearraHybridAutotuneDecision decision;

    metrics.cpu_confirm_queue_depth = 12u;
    metrics.cpu_buildup_queue_depth = 8u;
    decision = clearra_hybrid_autotune_evaluate(&budget, &metrics);

    EXPECT_TRUE(decision.selected_batch_size < budget.max_batch_size);
    EXPECT_TRUE(decision.prioritize_dedupe);
    EXPECT_TRUE(decision.defer_low_priority_candidates);
    EXPECT_U64(decision.throttle_reason,
               CLEARRA_HYBRID_THROTTLE_CPU_WORKER_QUEUE_DEPTH);
}
void autotune_throttles_when_readback_pending_high(void) {
    ClearraHybridAutotuneBudget budget =
        clearra_hybrid_autotune_budget_default();
    ClearraHybridAutotuneMetrics metrics = {0};
    ClearraHybridAutotuneDecision decision;

    metrics.gpu_readback_pending = budget.max_readback_pending + 1u;
    decision = clearra_hybrid_autotune_evaluate(&budget, &metrics);

    EXPECT_TRUE(decision.throttle_gpu_submission);
    EXPECT_U64(decision.throttle_reason,
               CLEARRA_HYBRID_THROTTLE_READBACK_PENDING);
}
void autotune_reports_memory_pressure(void) {
    ClearraHybridAutotuneBudget budget =
        clearra_hybrid_autotune_budget_default();
    ClearraHybridAutotuneMetrics metrics = {0};
    ClearraHybridAutotuneDecision decision;

    metrics.memory_ticket_live_count = budget.max_memory_pressure + 5u;
    metrics.pending_release_queue_depth = budget.max_memory_pressure + 1u;
    decision = clearra_hybrid_autotune_evaluate(&budget, &metrics);

    EXPECT_U64(decision.memory_pressure.level,
               CLEARRA_HYBRID_MEMORY_PRESSURE_HIGH);
    EXPECT_TRUE(decision.reduce_trace_retention);
    EXPECT_TRUE(decision.batch_scope_early_release);
}void memory_pressure_reduces_batch_size(void) {
    ClearraHybridAutotuneBudget budget =
        clearra_hybrid_autotune_budget_default();
    ClearraHybridAutotuneMetrics metrics = {0};
    ClearraHybridAutotuneDecision decision;

    metrics.memory_ticket_live_count = budget.max_memory_pressure + 5u;
    decision = clearra_hybrid_autotune_evaluate(&budget, &metrics);

    EXPECT_TRUE(decision.selected_batch_size < budget.max_batch_size);
    EXPECT_TRUE(decision.reduce_trace_retention);
    EXPECT_TRUE(decision.batch_scope_early_release);
    EXPECT_TRUE(decision.partial_result_diagnostic_required);
}void autotune_never_drops_coverage_rows_silently(void) {
    ClearraHybridAutotuneBudget budget =
        clearra_hybrid_autotune_budget_default();
    ClearraHybridAutotuneMetrics metrics = {0};
    ClearraHybridAutotuneDecision decision;

    metrics.coverage_row_buffer_pressure =
        budget.max_coverage_buffer_pressure + 1u;
    decision = clearra_hybrid_autotune_evaluate(&budget, &metrics);

    EXPECT_TRUE(decision.throttle_coverage_row_emission);
    EXPECT_TRUE(decision.count_only_mode_allowed);
    EXPECT_TRUE(decision.partial_result_diagnostic_required);
    EXPECT_TRUE(decision.truncation_reason != NULL);
}
void partial_result_reports_truncation_reason(void) {
    ClearraHybridAutotuneBudget budget =
        clearra_hybrid_autotune_budget_default();
    ClearraHybridAutotuneMetrics metrics = {0};
    ClearraHybridAutotuneDecision decision;

    metrics.memory_ticket_live_count = budget.max_memory_pressure + 1u;
    decision = clearra_hybrid_autotune_evaluate(&budget, &metrics);

    EXPECT_TRUE(decision.partial_result_diagnostic_required);
    EXPECT_TRUE(decision.truncation_reason != NULL);
}