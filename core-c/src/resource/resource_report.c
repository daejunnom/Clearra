#include "clr_resource_budget.h"
static size_t max_size(size_t left, size_t right) {
    return left > right ? left : right;
}void clr_resource_report_clear(clr_resource_report *report) {
    if (report == 0) {
        return;
    }
    report->truncated = 0u;
    report->probability_complete = 1u;
    report->truncation_reason = CLR_RESOURCE_TRUNCATION_NONE;
    report->peak_frontier_states = 0u;
    report->peak_candidate_rows = 0u;
    report->peak_hash_buckets = 0u;
    report->peak_gpu_bytes = 0u;
    report->peak_cpu_bytes = 0u;
    report->build_worker_backlog_peak = 0u;
    report->coverage_rows_emitted = 0u;
}void clr_resource_report_mark_truncated(
    clr_resource_report *report,
    uint16_t reason) {
    if (report == 0) {
        return;
    }
    report->truncated = 1u;
    report->probability_complete = 0u;
    if (report->truncation_reason == CLR_RESOURCE_TRUNCATION_NONE) {
        report->truncation_reason = reason;
    }
}void clr_resource_report_observe_frontier_states(
    clr_resource_report *report,
    size_t value) {
    if (report != 0) {
        report->peak_frontier_states =
            max_size(report->peak_frontier_states, value);
    }
}void clr_resource_report_observe_candidate_rows(
    clr_resource_report *report,
    size_t value) {
    if (report != 0) {
        report->peak_candidate_rows =
            max_size(report->peak_candidate_rows, value);
    }
}void clr_resource_report_observe_hash_buckets(
    clr_resource_report *report,
    size_t value) {
    if (report != 0) {
        report->peak_hash_buckets =
            max_size(report->peak_hash_buckets, value);
    }
}void clr_resource_report_observe_cpu_bytes(
    clr_resource_report *report,
    size_t value) {
    if (report != 0) {
        report->peak_cpu_bytes = max_size(report->peak_cpu_bytes, value);
    }
}void clr_resource_report_observe_coverage_rows(
    clr_resource_report *report,
    size_t value) {
    if (report != 0) {
        report->coverage_rows_emitted =
            max_size(report->coverage_rows_emitted, value);
    }
}