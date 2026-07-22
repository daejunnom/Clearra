#ifndef CLEARRA_BUILDUP_SEARCH_H
#define CLEARRA_BUILDUP_SEARCH_H

#include "buildup_internal.h"
#include "buildup_completion_memo.h"
#include "buildup_memo.h"
#include "buildup_reachability_result.h"
#include "buildup_operation_source.h"

typedef struct ClearraBuildUpRootTransitionCache
    ClearraBuildUpRootTransitionCache;
typedef struct ClearraBuildUpReachabilityCache
    ClearraBuildUpReachabilityCache;
typedef struct ClearraBuildUpReachableLockCache
    ClearraBuildUpReachableLockCache;
typedef struct ClearraBuildUpOperationVariantCache
    ClearraBuildUpOperationVariantCache;
typedef struct ClearraBuildUpGeometryTransitionCache
    ClearraBuildUpGeometryTransitionCache;
typedef struct ClearraBuildUpGeometryDag ClearraBuildUpGeometryDag;

typedef struct ClearraBuildUpSearchContext {
    const clr_buildup_problem *problem;
    ClearraBuildUpOperationSource operation_source;
    ClearraBuildUpRootTransitionCache *root_transition_cache;
    ClearraBuildUpReachabilityCache *reachability_cache;
    ClearraBuildUpReachableLockCache *reachable_lock_cache;
    ClearraReachabilityFrontier *reachability_frontier;
    ClearraBuildUpOperationVariantCache *operation_variant_cache;
    ClearraBuildUpGeometryTransitionCache *geometry_transition_cache;
    ClearraBuildUpGeometryDag *geometry_dag;
    uint64_t cache_identity_hash;
    ClearraBoard64Layout layout;
    ClearraBuildUpOrder order;
    ClearraBuildUpCompletionMemo completion_memo;
    const ClearraCompactRuleProfile *compiled_rule;
    ClearraCompactRuleProfile owned_compiled_rule;
    clr_buildup_status first_failure;
    uint16_t first_failure_step;
    ClearraBuildUpState success_state;
    uint8_t stop_after_first_success;
    uint8_t preserve_hold_branches;
    uint64_t max_count_variants;
    uint32_t max_retained_variants;
    clr_build_variant_buffer *out_variants;
    uint64_t enumerated_variant_count;
    uint64_t expanded_state_count;
    uint8_t incomplete_branch_seen;
    uint8_t fatal_branch_seen;
    uint8_t capture_trace;
    uint8_t reachability_trace_mode;
    clr_buildup_trace_step current_trace_steps[CLR_BUILDUP_MAX_OPERATIONS];
    clr_kick_evidence_view current_kick_evidence[CLR_BUILDUP_MAX_OPERATIONS];
    clr_buildup_trace_step success_trace_steps[CLR_BUILDUP_MAX_OPERATIONS];
    uint16_t success_operation_order_ids[CLR_BUILDUP_MAX_OPERATIONS];
    clr_kick_evidence_view
        success_kick_evidence[CLR_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT];
    uint16_t success_trace_step_count;
    uint16_t success_kick_evidence_count;
    uint32_t cancellation_poll_counter;
    uint8_t reachability_mode;
    uint8_t reserved1[7];
} ClearraBuildUpSearchContext;

clr_buildup_status clearra_buildup_verify_piece_window(
    const clr_buildup_problem *problem);
clr_buildup_status clearra_buildup_verify_piece_window_for_count(
    const clr_buildup_problem *problem,
    uint16_t operation_count);
clr_buildup_status clearra_buildup_search_context_init(
    const clr_buildup_problem *problem,
    ClearraBuildUpSearchContext *out_context);
clr_buildup_status clearra_buildup_search_context_init_with_reachability(
    const clr_buildup_problem *problem,
    const ClearraCompactRuleProfile *compiled_rule,
    ClearraBuildUpSearchContext *out_context);
clr_buildup_status clearra_buildup_search_context_init_catalog_rows(
    const clr_buildup_problem *problem,
    const ClearraGeometryCatalog *catalog,
    const uint32_t *row_ids,
    uint16_t operation_count,
    const uint8_t *representative_order_hint,
    const uint16_t *required_predecessors,
    const ClearraCompactRuleProfile *compiled_rule,
    ClearraBuildUpSearchContext *out_context);
clr_buildup_status clearra_buildup_search_order(
    ClearraBuildUpSearchContext *context,
    ClearraBuildUpState state,
    ClearraBuildUpQueueHold queue_hold,
    uint16_t remaining_operations,
    uint16_t depth);
#endif
