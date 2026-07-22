#ifndef CLR_SEARCH_PROFILE_H
#define CLR_SEARCH_PROFILE_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define CLR_SEARCH_PROFILE_MAX_PACKING_DEPTH 16u

typedef enum clr_search_profile_stage {
    CLR_PROFILE_SUPPLY_MULTISET_FAMILY = 0,
    CLR_PROFILE_PACKING_TOTAL,
    CLR_PROFILE_PACKING_VALIDATE_AND_LOWER,
    CLR_PROFILE_PACKING_CONTEXT_ALLOCATE,
    CLR_PROFILE_PACKING_OUTPUT_CLEAR,
    CLR_PROFILE_PACKING_OPERATION_TABLES,
    CLR_PROFILE_PACKING_SUPPORT_INDEX_BUILD,
    CLR_PROFILE_PACKING_PULL_CELL_SELECT,
    CLR_PROFILE_PACKING_DEPTH_EXPAND,
    CLR_PROFILE_PACKING_FRONTIER_BUCKET_INDEX_CLEAR,
    CLR_PROFILE_PACKING_FRONTIER_EXACT_REDUCE,
    CLR_PROFILE_PACKING_FRONTIER_GROW,
    CLR_PROFILE_PACKING_FRONTIER_SWAP,
    CLR_PROFILE_PACKING_DEPTH_EMIT,
    CLR_PROFILE_PACKING_CANDIDATE_CANONICALIZE,
    CLR_PROFILE_PACKING_CANDIDATE_DEDUPE,
    CLR_PROFILE_PACKING_PIECE_DOMAIN_SKIPS,
    CLR_PROFILE_PACKING_PULL_SUPPORT_CANDIDATES,
    CLR_PROFILE_PACKING_STATIC_PRUNE_CALLS,
    CLR_PROFILE_PACKING_STATIC_PRUNE_REJECTS,
    CLR_PROFILE_PACKING_MULTISET_PREFIX_CALLS,
    CLR_PROFILE_PACKING_MULTISET_PREFIX_REJECTS,
    CLR_PROFILE_PACKING_CHILD_APPENDS,
    CLR_PROFILE_PACKING_CONTEXT_RELEASE,
    CLR_PROFILE_BUILDUP_TOTAL,
    CLR_PROFILE_BUILDUP_EXISTS,
    CLR_PROFILE_BUILDUP_VALIDATE,
    CLR_PROFILE_BUILDUP_QUEUE_HOLD_INIT,
    CLR_PROFILE_BUILDUP_SEARCH,
    CLR_PROFILE_BUILDUP_MEMO_LOOKUP,
    CLR_PROFILE_BUILDUP_HOLD_BRANCH_ENUMERATION,
    CLR_PROFILE_BUILDUP_OPERATION_VARIANT_CACHE_LOOKUPS,
    CLR_PROFILE_BUILDUP_OPERATION_VARIANT_CACHE_HITS,
    CLR_PROFILE_BUILDUP_OPERATION_VARIANT_GENERATION,
    CLR_PROFILE_BUILDUP_GEOMETRY_TRANSITION_CACHE_LOOKUPS,
    CLR_PROFILE_BUILDUP_GEOMETRY_TRANSITION_CACHE_HITS,
    CLR_PROFILE_BUILDUP_Y_ADJUSTMENT,
    CLR_PROFILE_BUILDUP_LINE_DEPENDENCY,
    CLR_PROFILE_BUILDUP_GROUNDED,
    CLR_PROFILE_BUILDUP_REACHABILITY_CACHE_LOOKUPS,
    CLR_PROFILE_BUILDUP_REACHABILITY_CACHE_HITS,
    CLR_PROFILE_BUILDUP_REACHABILITY,
    CLR_PROFILE_BUILDUP_PLACE_AND_CLEAR,
    CLR_PROFILE_BUILDUP_LINE_STATE_UPDATE,
    CLR_PROFILE_BUILDUP_MEMO_INSERT,
    CLR_PROFILE_BUILDUP_REALIZATION_FEASIBILITY,
    CLR_PROFILE_BUILDUP_REALIZATION_FEASIBLE,
    CLR_PROFILE_BUILDUP_REALIZATION_INFEASIBLE,
    CLR_PROFILE_BUILDUP_REALIZATION_UNKNOWN,
    CLR_PROFILE_PACKING_GEOMETRY_RESIDUAL_MEMO_LOOKUPS,
    CLR_PROFILE_PACKING_GEOMETRY_RESIDUAL_MEMO_HITS,
    CLR_PROFILE_PACKING_GEOMETRY_COMPONENT_COMPOSITIONS,
    CLR_PROFILE_BUILDUP_CLEAR_STATE_SKIPS,
    CLR_PROFILE_STAGE_COUNT
} clr_search_profile_stage;

typedef struct clr_search_profile_span {
    uint64_t started_ns;
    uint16_t stage;
    uint8_t active;
    uint8_t reserved[5];
} clr_search_profile_span;

typedef struct clr_search_stage_profile {
    uint8_t enabled;
    uint8_t reserved[7];
    uint64_t duration_ns[CLR_PROFILE_STAGE_COUNT];
    uint64_t invocation_count[CLR_PROFILE_STAGE_COUNT];
    uint64_t work_item_count[CLR_PROFILE_STAGE_COUNT];
    uint64_t packing_depth_expand_ns[CLR_SEARCH_PROFILE_MAX_PACKING_DEPTH];
    uint64_t packing_depth_reduce_ns[CLR_SEARCH_PROFILE_MAX_PACKING_DEPTH];
    uint64_t packing_depth_emit_ns[CLR_SEARCH_PROFILE_MAX_PACKING_DEPTH];
    uint64_t packing_depth_frontier_in[CLR_SEARCH_PROFILE_MAX_PACKING_DEPTH];
    uint64_t packing_depth_frontier_out[CLR_SEARCH_PROFILE_MAX_PACKING_DEPTH];
    uint64_t packing_depth_candidate_count[CLR_SEARCH_PROFILE_MAX_PACKING_DEPTH];
    uint8_t packing_depth_incomplete[CLR_SEARCH_PROFILE_MAX_PACKING_DEPTH];
} clr_search_stage_profile;

#if defined(CLEARRA_ENABLE_STAGE_PROFILING)
void clr_search_stage_profile_init(clr_search_stage_profile *profile);
bool clr_search_stage_profile_activate(clr_search_stage_profile *profile);
void clr_search_stage_profile_deactivate(clr_search_stage_profile *profile);
clr_search_profile_span clr_search_profile_begin(clr_search_profile_stage stage);
uint64_t clr_search_profile_end(clr_search_profile_span span, uint64_t work_items);
void clr_search_profile_count(clr_search_profile_stage stage, uint64_t work_items);
void clr_search_profile_observe_packing_depth(
    uint8_t depth,
    uint64_t frontier_in,
    uint64_t frontier_out,
    uint64_t expand_ns,
    uint64_t reduce_ns);
void clr_search_profile_observe_packing_depth_incomplete(
    uint8_t depth,
    uint64_t frontier_in,
    uint64_t frontier_out,
    uint64_t expand_ns);
void clr_search_profile_observe_packing_emit(
    uint8_t depth,
    uint64_t candidate_count,
    uint64_t emit_ns);
#else
static inline void clr_search_stage_profile_init(clr_search_stage_profile *profile) {
    (void)profile;
}
static inline bool clr_search_stage_profile_activate(clr_search_stage_profile *profile) {
    (void)profile;
    return false;
}
static inline void clr_search_stage_profile_deactivate(clr_search_stage_profile *profile) {
    (void)profile;
}
static inline clr_search_profile_span clr_search_profile_begin(
    clr_search_profile_stage stage) {
    (void)stage;
    return (clr_search_profile_span){0};
}
static inline uint64_t clr_search_profile_end(
    clr_search_profile_span span,
    uint64_t work_items) {
    (void)span;
    (void)work_items;
    return 0u;
}
static inline void clr_search_profile_count(
    clr_search_profile_stage stage,
    uint64_t work_items) {
    (void)stage;
    (void)work_items;
}
static inline void clr_search_profile_observe_packing_depth(
    uint8_t depth,
    uint64_t frontier_in,
    uint64_t frontier_out,
    uint64_t expand_ns,
    uint64_t reduce_ns) {
    (void)depth;
    (void)frontier_in;
    (void)frontier_out;
    (void)expand_ns;
    (void)reduce_ns;
}
static inline void clr_search_profile_observe_packing_depth_incomplete(
    uint8_t depth,
    uint64_t frontier_in,
    uint64_t frontier_out,
    uint64_t expand_ns) {
    (void)depth;
    (void)frontier_in;
    (void)frontier_out;
    (void)expand_ns;
}
static inline void clr_search_profile_observe_packing_emit(
    uint8_t depth,
    uint64_t candidate_count,
    uint64_t emit_ns) {
    (void)depth;
    (void)candidate_count;
    (void)emit_ns;
}
#endif

const char *clr_search_profile_stage_name(clr_search_profile_stage stage);
clr_search_stage_profile *clr_search_stage_profile_create(void);
void clr_search_stage_profile_release(clr_search_stage_profile *profile);
bool clr_search_stage_profile_start(clr_search_stage_profile *profile);
void clr_search_stage_profile_stop(clr_search_stage_profile *profile);
size_t clr_search_stage_profile_stage_count(void);
uint64_t clr_search_stage_profile_duration_ns(
    const clr_search_stage_profile *profile,
    size_t stage);
uint64_t clr_search_stage_profile_invocation_count(
    const clr_search_stage_profile *profile,
    size_t stage);
uint64_t clr_search_stage_profile_work_item_count(
    const clr_search_stage_profile *profile,
    size_t stage);

#endif
