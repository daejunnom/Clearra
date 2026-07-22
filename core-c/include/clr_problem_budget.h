#ifndef CLR_PROBLEM_BUDGET_H
#define CLR_PROBLEM_BUDGET_H

#include <stdint.h>
typedef struct clr_problem_budget {
    uint64_t max_nodes;
    uint32_t max_seconds;
    uint32_t max_results;
    uint32_t max_patterns;
    uint32_t max_frontier_states;
    uint32_t max_memory_mib;
    uint8_t has_max_memory_mib;
    uint8_t reserved[7];
} clr_problem_budget;typedef struct clr_backend_request {
    uint32_t requested_backend;
    uint16_t workers;
    uint8_t deterministic;
    uint8_t reserved_flags;
    uint8_t fallback_policy;
    uint8_t gpu_device_kind;
    uint8_t gpu_device_index;
    uint8_t reserved;
} clr_backend_request;typedef struct clr_checkpoint_spec {
    uint16_t label_count;
    uint16_t checkpoint_count;
    uint16_t partition_count;
    uint16_t reserved;
} clr_checkpoint_spec;clr_problem_budget clr_problem_budget_zero(void);
clr_checkpoint_spec clr_checkpoint_spec_none(void);
#endif
