#include "clr_resource_budget.h"
clr_resource_budget clr_resource_budget_default(void) {
    clr_resource_budget budget;
    budget.max_frontier_states = 2048u;
    budget.max_candidate_rows = 8192u;
    budget.max_hash_buckets = 8192u;
    budget.max_gpu_batch_bytes = 64u * 1024u * 1024u;
    budget.max_readback_bytes = 64u * 1024u * 1024u;
    budget.max_build_worker_backlog = 4096u;
    budget.max_coverage_rows = 2048u;
    budget.max_pattern_bits = 1048576u;
    budget.max_cpu_time_per_batch_ms = 1000u;
    budget.max_memory_mib = 0u;
    budget.has_max_memory_mib = 0u;
    budget.reserved[0] = 0u;
    budget.reserved[1] = 0u;
    budget.reserved[2] = 0u;
    return budget;
}bool clr_resource_budget_is_valid(const clr_resource_budget *budget) {
    return budget != 0 && budget->max_frontier_states > 0u &&
           budget->max_candidate_rows > 0u && budget->max_hash_buckets > 0u &&
           budget->max_gpu_batch_bytes > 0u &&
           budget->max_readback_bytes > 0u &&
           budget->max_build_worker_backlog > 0u &&
           budget->max_coverage_rows > 0u && budget->max_pattern_bits > 0u &&
           budget->max_cpu_time_per_batch_ms > 0u;
}
