#include "scheduler_tests_support.h"

void cpu_only_result_equals_hybrid_result(void);
void hybrid_cpu_fallback_returns_confirmed_candidates_for_product_path(void);
void gpu_only_packing_plus_cpu_buildup_matches_cpu_reference(void);
void gpu_confirmed_candidate_builds_variant_only_after_cpu_buildup(void);
void coverage_row_created_only_after_buildup_acceptance(void);
void gpu_assisted_opening_2l_reaches_buildup(void);
void gpu_assisted_buildvariant_count_matches_cpu_reference(void);
void gpu_assisted_coverage_rows_match_cpu_reference(void);
void gpu_verify_first_not_used_for_coverage(void);
void hybrid_collect_uses_piece_source_pattern_id_not_candidate_index(void);
void hybrid_collect_rejects_kick_evidence_count_over_limit(void);
void backend_metrics_reported(void);
void hybrid_result_reports_backend_metrics(void);
void hybrid_cpu_fallback_reports_no_gpu_throttle(void);
void hybrid_scheduler_reports_cpu_fallback_backpressure_contract(void);
void hybrid_scheduler_cpu_fallback_does_not_submit_gpu_worker(void);
void hybrid_cpu_fallback_reports_zero_gpu_queue_depth(void);
void hybrid_cpu_fallback_reports_zero_readback_pending(void);
void hybrid_scheduler_throttles_when_cpu_buildup_backlog_high(void);
void hybrid_scheduler_throttles_when_coverage_buffer_pressure_high(void);
void hybrid_gpu_queue_tracks_submitted_completed_and_latency(void);
void hybrid_readback_queue_tracks_pending_and_candidate_pressure(void);
void hybrid_cpu_confirm_queue_tracks_confirm_and_buildup_depth(void);
void hybrid_cpu_fallback_metrics_exclude_gpu_queue_stats(void);
void autotune_reduces_batch_size_when_cpu_backlog_high(void);
void autotune_throttles_when_readback_pending_high(void);
void autotune_reports_memory_pressure(void);
void memory_pressure_reduces_batch_size(void);
void autotune_never_drops_coverage_rows_silently(void);
void partial_result_reports_truncation_reason(void);
void fallback_reason_reported(void);
void hybrid_scheduler_fallback_reports_reason(void);
void memory_leak_report_clean(void);
void hybrid_scheduler_uses_scope_allocator_for_scratch_buffers(void);
void hybrid_scheduler_no_raw_malloc_in_hot_path(void);
void hybrid_scheduler_failure_has_clean_leak_report(void);
void no_fallback_reports_unavailable_without_cpu_work(void);

#define RUN_SCHEDULER_TEST(test_fn)  \
    do {                             \
        fputs(#test_fn "\n", stdout); \
        fflush(stdout);              \
        test_fn();                   \
    } while (0)

int main(void) {
    RUN_SCHEDULER_TEST(cpu_only_result_equals_hybrid_result);
    RUN_SCHEDULER_TEST(hybrid_cpu_fallback_returns_confirmed_candidates_for_product_path);
    RUN_SCHEDULER_TEST(gpu_only_packing_plus_cpu_buildup_matches_cpu_reference);
    RUN_SCHEDULER_TEST(gpu_confirmed_candidate_builds_variant_only_after_cpu_buildup);
    RUN_SCHEDULER_TEST(coverage_row_created_only_after_buildup_acceptance);
    RUN_SCHEDULER_TEST(gpu_assisted_opening_2l_reaches_buildup);
    RUN_SCHEDULER_TEST(gpu_assisted_buildvariant_count_matches_cpu_reference);
    RUN_SCHEDULER_TEST(gpu_assisted_coverage_rows_match_cpu_reference);
    RUN_SCHEDULER_TEST(gpu_verify_first_not_used_for_coverage);
    RUN_SCHEDULER_TEST(hybrid_collect_uses_piece_source_pattern_id_not_candidate_index);
    RUN_SCHEDULER_TEST(hybrid_collect_rejects_kick_evidence_count_over_limit);
    RUN_SCHEDULER_TEST(backend_metrics_reported);
    RUN_SCHEDULER_TEST(hybrid_result_reports_backend_metrics);
    RUN_SCHEDULER_TEST(hybrid_cpu_fallback_reports_no_gpu_throttle);
    RUN_SCHEDULER_TEST(hybrid_scheduler_reports_cpu_fallback_backpressure_contract);
    RUN_SCHEDULER_TEST(hybrid_scheduler_cpu_fallback_does_not_submit_gpu_worker);
    RUN_SCHEDULER_TEST(hybrid_cpu_fallback_reports_zero_gpu_queue_depth);
    RUN_SCHEDULER_TEST(hybrid_cpu_fallback_reports_zero_readback_pending);
    RUN_SCHEDULER_TEST(hybrid_scheduler_throttles_when_cpu_buildup_backlog_high);
    RUN_SCHEDULER_TEST(hybrid_scheduler_throttles_when_coverage_buffer_pressure_high);
    RUN_SCHEDULER_TEST(hybrid_gpu_queue_tracks_submitted_completed_and_latency);
    RUN_SCHEDULER_TEST(hybrid_readback_queue_tracks_pending_and_candidate_pressure);
    RUN_SCHEDULER_TEST(hybrid_cpu_confirm_queue_tracks_confirm_and_buildup_depth);
    RUN_SCHEDULER_TEST(hybrid_cpu_fallback_metrics_exclude_gpu_queue_stats);
    RUN_SCHEDULER_TEST(autotune_reduces_batch_size_when_cpu_backlog_high);
    RUN_SCHEDULER_TEST(autotune_throttles_when_readback_pending_high);
    RUN_SCHEDULER_TEST(autotune_reports_memory_pressure);
    RUN_SCHEDULER_TEST(memory_pressure_reduces_batch_size);
    RUN_SCHEDULER_TEST(autotune_never_drops_coverage_rows_silently);
    RUN_SCHEDULER_TEST(partial_result_reports_truncation_reason);
    RUN_SCHEDULER_TEST(fallback_reason_reported);
    RUN_SCHEDULER_TEST(hybrid_scheduler_fallback_reports_reason);
    RUN_SCHEDULER_TEST(memory_leak_report_clean);
    RUN_SCHEDULER_TEST(hybrid_scheduler_uses_scope_allocator_for_scratch_buffers);
    RUN_SCHEDULER_TEST(hybrid_scheduler_no_raw_malloc_in_hot_path);
    RUN_SCHEDULER_TEST(hybrid_scheduler_failure_has_clean_leak_report);
    RUN_SCHEDULER_TEST(no_fallback_reports_unavailable_without_cpu_work);
    puts("core-c scheduler tests passed");
    return 0;
}
