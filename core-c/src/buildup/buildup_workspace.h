#ifndef CLEARRA_BUILDUP_WORKSPACE_H
#define CLEARRA_BUILDUP_WORKSPACE_H

#include "buildup_completion_memo.h"
#include "buildup_geometry_dag.h"
#include "buildup_geometry_transition_cache.h"
#include "buildup_operation_variant_cache.h"
#include "buildup_reachability_cache.h"
#include "buildup_reachable_lock_cache.h"
#include "buildup_reachability_result.h"
#include "realization_feasibility.h"
#include "buildup_state.h"
#include "../reachability/locked_reachability_internal.h"

typedef struct ClearraBuildUpSearchContext ClearraBuildUpSearchContext;

typedef struct ClearraBuildUpRootTransition {
    clr_buildup_status status;
    ClearraBuildUpState next_state;
    clr_buildup_trace_step trace_step;
    clr_kick_evidence_view kick_evidence;
} ClearraBuildUpRootTransition;

typedef struct ClearraBuildUpRootOperationTransitions {
    clr_buildup_status preparation_status;
    uint32_t generation;
    uint8_t count;
    uint8_t reserved[3];
    ClearraBuildUpRootTransition
        transitions[CLR_BUILDUP_MAX_OPERATION_VARIANTS];
} ClearraBuildUpRootOperationTransitions;

typedef struct ClearraBuildUpRootTransitionCache {
    clr_board_descriptor initial_board;
    clr_buildup_operation_set operation_set;
    uint64_t candidate_id;
    uint64_t canonical_operation_set_id;
    ClearraBuildUpRootOperationTransitions
        operations[CLR_BUILDUP_MAX_OPERATIONS];
    uint32_t generation;
    uint8_t prepared;
    uint8_t capture_trace;
    uint8_t reachability_trace_mode;
    uint8_t reserved;
} ClearraBuildUpRootTransitionCache;

struct clr_buildup_workspace {
    ClearraCompactRuleProfile compiled_rule;
    clr_rule_profile_descriptor rule_descriptor;
    ClearraBoard64Layout layout;
    ClearraBuildUpCompletionMemoStorage completion_memo_storage;
    ClearraBuildUpRootTransitionCache root_transition_cache;
    ClearraBuildUpOperationVariantCache operation_variant_cache;
    ClearraBuildUpReachabilityCache reachability_cache;
    ClearraBuildUpReachableLockCache reachable_lock_cache;
    ClearraReachabilityFrontier reachability_frontier;
    ClearraBuildUpGeometryTransitionCache geometry_transition_cache;
    ClearraBuildUpGeometryDag geometry_dag;
    ClearraRealizationFeasibilityWorkspace realization_feasibility;
    uint8_t reachability_mode;
    uint8_t initialized;
    uint8_t reserved[6];
};

clr_buildup_status clearra_buildup_workspace_prepare(
    clr_buildup_workspace *workspace,
    const clr_buildup_problem *problem);
clr_buildup_status clearra_buildup_exists_catalog_rows_with_workspace(
    const clr_buildup_problem *problem,
    const ClearraGeometryCatalog *catalog,
    const uint32_t *row_ids,
    uint16_t operation_count,
    const uint8_t *representative_order_hint,
    clr_buildup_workspace *workspace);
clr_buildup_status
clearra_buildup_exists_catalog_rows_with_constraints_and_workspace(
    const clr_buildup_problem *problem,
    const ClearraGeometryCatalog *catalog,
    const uint32_t *row_ids,
    uint16_t operation_count,
    const uint8_t *representative_order_hint,
    const uint16_t *required_predecessors,
    clr_buildup_workspace *workspace);
clr_buildup_status clearra_buildup_root_transition_cache_prepare_operation(
    ClearraBuildUpSearchContext *context,
    const ClearraBuildUpState *initial_state,
    uint16_t operation_index,
    ClearraBuildUpRootOperationTransitions *operation_cache);

#endif
