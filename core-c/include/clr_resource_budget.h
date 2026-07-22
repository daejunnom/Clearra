#ifndef CLR_RESOURCE_BUDGET_H
#define CLR_RESOURCE_BUDGET_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define CLR_RESOURCE_TRUNCATION_NONE 0u
#define CLR_RESOURCE_TRUNCATION_FRONTIER_BUDGET_EXCEEDED 1u
#define CLR_RESOURCE_TRUNCATION_CANDIDATE_BUDGET_EXCEEDED 2u
#define CLR_RESOURCE_TRUNCATION_HASH_BUCKET_BUDGET_EXCEEDED 3u
#define CLR_RESOURCE_TRUNCATION_GPU_BATCH_BYTES_EXCEEDED 4u
#define CLR_RESOURCE_TRUNCATION_READBACK_BYTES_EXCEEDED 5u
#define CLR_RESOURCE_TRUNCATION_BUILD_WORKER_BACKLOG_EXCEEDED 6u
#define CLR_RESOURCE_TRUNCATION_COVERAGE_ROWS_EXCEEDED 7u
#define CLR_RESOURCE_TRUNCATION_PATTERN_BITS_EXCEEDED 8u
#define CLR_RESOURCE_TRUNCATION_CPU_TIME_EXCEEDED 9u
#define CLR_RESOURCE_TRUNCATION_MEMORY_EXCEEDED 10u
#define CLR_RESOURCE_TRUNCATION_OBSERVED_UNIVERSE_TRUNCATED 11u
#define CLR_RESOURCE_TRUNCATION_CANCELLED 12u
#define CLR_RESOURCE_TRUNCATION_OPERATION_TABLE_CAPACITY_EXCEEDED 13u
#define CLR_RESOURCE_TRUNCATION_PRUNING_EVIDENCE_CAPACITY_EXCEEDED 14u
typedef struct clr_resource_budget {
    size_t max_frontier_states;
    size_t max_candidate_rows;
    size_t max_hash_buckets;
    size_t max_gpu_batch_bytes;
    size_t max_readback_bytes;
    size_t max_build_worker_backlog;
    size_t max_coverage_rows;
    size_t max_pattern_bits;
    uint64_t max_cpu_time_per_batch_ms;
    uint32_t max_memory_mib;
    uint8_t has_max_memory_mib;
    uint8_t reserved[3];
} clr_resource_budget;typedef struct clr_resource_report {
    uint8_t truncated;
    uint8_t probability_complete;
    uint16_t truncation_reason;
    size_t peak_frontier_states;
    size_t peak_candidate_rows;
    size_t peak_hash_buckets;
    size_t peak_gpu_bytes;
    size_t peak_cpu_bytes;
    size_t build_worker_backlog_peak;
    size_t coverage_rows_emitted;
} clr_resource_report;clr_resource_budget clr_resource_budget_default(void);
bool clr_resource_budget_is_valid(const clr_resource_budget *budget);
void clr_resource_report_clear(clr_resource_report *report);
void clr_resource_report_mark_truncated(
    clr_resource_report *report,
    uint16_t reason);
void clr_resource_report_observe_frontier_states(
    clr_resource_report *report,
    size_t value);
void clr_resource_report_observe_candidate_rows(
    clr_resource_report *report,
    size_t value);
void clr_resource_report_observe_hash_buckets(
    clr_resource_report *report,
    size_t value);
void clr_resource_report_observe_cpu_bytes(
    clr_resource_report *report,
    size_t value);
void clr_resource_report_observe_coverage_rows(
    clr_resource_report *report,
    size_t value);
#endif
