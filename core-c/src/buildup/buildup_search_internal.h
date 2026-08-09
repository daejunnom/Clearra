#ifndef CLEARRA_BUILDUP_SEARCH_INTERNAL_H
#define CLEARRA_BUILDUP_SEARCH_INTERNAL_H

#include "buildup_search.h"
clr_buildup_status clearra_buildup_search_layout_from_problem(
    const clr_buildup_problem *problem,
    ClearraBoard64Layout *out_layout);
clr_buildup_status clearra_buildup_search_verify_goal(
    const clr_buildup_problem *problem,
    const ClearraBuildUpState *state);
void clearra_buildup_search_record_failure(
    ClearraBuildUpSearchContext *context,
    clr_buildup_status status,
    uint16_t step);
bool clearra_buildup_search_failed_memo_contains(
    ClearraBuildUpSearchContext *context,
    const ClearraBuildUpState *state,
    uint16_t remaining_operations);
void clearra_buildup_search_failed_memo_insert(
    ClearraBuildUpSearchContext *context,
    const ClearraBuildUpState *state,
    uint16_t remaining_operations);
bool clearra_buildup_search_completion_memo_lookup(
    ClearraBuildUpSearchContext *context,
    const ClearraBuildUpState *state,
    uint16_t remaining_operations,
    uint64_t *out_completion_count);
void clearra_buildup_search_completion_memo_insert(
    ClearraBuildUpSearchContext *context,
    const ClearraBuildUpState *state,
    uint16_t remaining_operations,
    uint64_t completion_count);
clr_buildup_status clearra_buildup_search_update_line_clear_state(
    ClearraBuildUpState *state,
    ClearraBoard64LineClearResult clear_result);
void clearra_buildup_trace_step_from_operation(
    const clr_buildup_problem *problem,
    const clr_buildup_operation *operation,
    uint16_t operation_index,
    int8_t adjusted_y,
    ClearraBoard64LineClearResult clear_result,
    const ClearraBuildUpReachabilityResult *reachability,
    clr_buildup_trace_step *out_step,
    clr_kick_evidence_view *out_kick_evidence);
uint64_t clearra_buildup_trace_identity(
    const clr_buildup_trace_step *steps,
    uint16_t step_count);
uint64_t clearra_buildup_trace_operation_set_hash(
    const clr_buildup_trace_step *steps,
    uint16_t step_count);
void clearra_buildup_apply_kick_trace_completeness(
    const clr_buildup_trace_step *steps,
    uint16_t step_count,
    clr_build_variant_view *variant);
void clearra_buildup_capture_success_path(
    ClearraBuildUpSearchContext *context,
    uint16_t step_count);
void clearra_buildup_attach_success_trace(
    const ClearraBuildUpSearchContext *context,
    clr_build_variant_view *variant);
void clearra_buildup_copy_success_trace_to_verification(
    const ClearraBuildUpSearchContext *context,
    clr_buildup_verification *verification);
typedef struct ClearraBuildUpGeometryTransitionView {
    uint64_t target_mask;
    uint16_t cleared_row_mask;
    int8_t adjusted_y;
    uint8_t cleared_lines;
} ClearraBuildUpGeometryTransitionView;
clr_buildup_status clearra_buildup_search_try_operation(
    ClearraBuildUpSearchContext *context,
    ClearraBuildUpState state,
    ClearraBuildUpQueueHold queue_hold,
    const clr_buildup_operation *operation,
    uint16_t operation_index,
    ClearraBuildUpState *out_next_state,
    clr_buildup_trace_step *out_trace_step,
    clr_kick_evidence_view *out_kick_evidence);
clr_buildup_status clearra_buildup_search_try_operation_with_geometry(
    ClearraBuildUpSearchContext *context,
    ClearraBuildUpState state,
    ClearraBuildUpQueueHold queue_hold,
    const clr_buildup_operation *operation,
    uint16_t operation_index,
    ClearraBuildUpState *out_next_state,
    clr_buildup_trace_step *out_trace_step,
    clr_kick_evidence_view *out_kick_evidence,
    ClearraBuildUpGeometryTransitionView *out_geometry);
clr_buildup_status clearra_buildup_operation_variants_for_state(
    const ClearraBuildUpSearchContext *context,
    const ClearraBuildUpState *state,
    uint16_t operation_index,
    clr_buildup_operation out_variants[CLR_BUILDUP_MAX_OPERATION_VARIANTS],
    uint8_t *out_count);
clr_buildup_status clearra_buildup_search_record_success(
    ClearraBuildUpSearchContext *context,
    const ClearraBuildUpState *state);
#endif
