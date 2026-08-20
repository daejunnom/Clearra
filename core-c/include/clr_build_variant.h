#ifndef CLR_BUILD_VARIANT_H
#define CLR_BUILD_VARIANT_H

#define CLR_BUILDUP_HOLD_BRANCH_CURRENT 1u
#define CLR_BUILDUP_HOLD_BRANCH_SWAP_HELD 2u
#define CLR_BUILDUP_HOLD_BRANCH_STORE_CURRENT 3u
#define CLR_BUILDUP_HOLD_BRANCH_RELEASE_HELD_AT_TERMINAL 4u

#include "clr_buildup_problem.h"
#include <stddef.h>
typedef struct clr_kick_evidence_view {
    uint8_t has_kick_evidence;
    uint8_t from_rotation;
    uint8_t to_rotation;
    uint8_t rotation_request;
    uint8_t kick_index;
    int8_t kick_dx;
    int8_t kick_dy;
    uint8_t reserved0;
    uint64_t kick_table_id;
    uint64_t kick_profile_id;
    uint8_t first_success_confirmed;
    uint8_t reserved1[7];
    int16_t predecessor_x;
    int16_t predecessor_y;
    int16_t result_x;
    int16_t result_y;
} clr_kick_evidence_view;typedef struct clr_reachability_evidence_view {
    uint8_t reachable;
    uint8_t exhaustive;
    uint8_t used_kick;
    uint8_t used_180;
    uint16_t visited_states;
    uint8_t last_action_was_rotation;
    uint8_t rotation_evidence_complete;
    uint64_t path_digest;
} clr_reachability_evidence_view;typedef struct clr_buildup_trace_step {
    uint16_t operation_id;
    uint16_t operation_index;
    uint8_t piece;
    uint8_t rotation;
    uint8_t hold_branch_kind;
    uint8_t used_hold;
    uint8_t incoming_piece;
    uint8_t held_piece_before;
    uint8_t hold_empty_before;
    uint8_t kick_evidence_index;
    int8_t adjusted_x;
    int8_t adjusted_y;
    uint16_t cleared_row_mask;
    uint64_t target_frame_mask;
    clr_reachability_evidence_view reachability;
} clr_buildup_trace_step;typedef struct clr_build_variant_view {
    uint64_t candidate_id;
    uint64_t build_variant_id;
    uint64_t canonical_operation_set_id;
    uint64_t operation_set_hash;
    uint64_t final_board;
    uint32_t coverage_pattern_id;
    uint16_t placed_count;
    uint16_t queue_cursor;
    uint8_t hold_piece;
    uint8_t hold_empty;
    uint8_t cleared_lines;
    uint8_t hold_branch_kind;
    uint64_t trace_identity;
    const uint16_t *operation_order_ids;
    const clr_buildup_trace_step *trace_steps;
    uint16_t operation_order_count;
    uint16_t trace_step_count;
    const clr_kick_evidence_view *kick_evidence;
    uint32_t kick_evidence_count;
    uint32_t trace_completeness_flags;
} clr_build_variant_view;typedef struct clr_buildup_search_metrics {
    uint64_t expanded_state_count;
    uint64_t memo_probes;
    uint64_t memo_hits;
    uint64_t memo_insertions;
    uint64_t memo_saturation_skips;
    uint32_t memo_capacity;
    uint32_t memo_max_probe_length;
} clr_buildup_search_metrics;typedef struct clr_build_variant_buffer {
    uint16_t count;
    uint16_t reserved;
    uint64_t total_variant_count;
    uint8_t count_complete;
    uint8_t trace_retention_truncated;
    uint8_t reserved2[6];
    clr_buildup_search_metrics search_metrics;
    clr_build_variant_view variants[CLR_BUILDUP_MAX_VARIANTS];
    clr_kick_evidence_view
        kick_evidence_storage[CLR_BUILDUP_MAX_VARIANTS]
                             [CLR_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT];
    uint16_t operation_order_storage[CLR_BUILDUP_MAX_VARIANTS]
                                    [CLR_BUILDUP_MAX_OPERATIONS];
    clr_buildup_trace_step trace_step_storage[CLR_BUILDUP_MAX_VARIANTS]
                                               [CLR_BUILDUP_MAX_OPERATIONS];
} clr_build_variant_buffer;typedef struct clr_buildup_enumeration_limits {
    uint32_t max_variants;
    uint8_t preserve_hold_branches;
    uint8_t prefer_highest_t_spin_trace;
    uint8_t reserved[6];
} clr_buildup_enumeration_limits;typedef struct clr_buildup_count_limits {
    uint32_t max_variants;
    uint8_t preserve_hold_branches;
    uint8_t retain_traces;
    uint8_t reserved[6];
} clr_buildup_count_limits;typedef struct clr_buildup_count_report {
    uint64_t total_variant_count;
    uint8_t search_complete;
    uint8_t solution_exists;
    uint8_t count_complete;
    uint8_t trace_retained;
    uint16_t retained_variant_count;
    uint16_t reserved;
    uint32_t no_variant_reason;
    uint32_t truncation_reason;
    clr_buildup_search_metrics search_metrics;
} clr_buildup_count_report;typedef struct clr_buildup_verification {
    uint8_t accepted;
    uint8_t reserved;
    uint16_t rejected_step;
    uint32_t reject_reason;
    clr_build_variant_view variant;
    clr_kick_evidence_view
        kick_evidence_storage[CLR_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT];
    uint16_t operation_order_storage[CLR_BUILDUP_MAX_OPERATIONS];
    clr_buildup_trace_step trace_step_storage[CLR_BUILDUP_MAX_OPERATIONS];
} clr_buildup_verification;void clr_build_variant_buffer_clear(clr_build_variant_buffer *buffer);
typedef struct clr_buildup_workspace clr_buildup_workspace;
clr_buildup_workspace *clr_buildup_workspace_create(void);
void clr_buildup_workspace_release(clr_buildup_workspace *workspace);
size_t clr_buildup_workspace_retained_bytes(
    const clr_buildup_workspace *workspace);
clr_buildup_status clr_build_variant_buffer_push_verified(
    clr_build_variant_buffer *buffer,
    const clr_buildup_verification *verification);
clr_buildup_status clr_buildup_worker_verify(
    const clr_buildup_problem *problem,
    clr_buildup_verification *out_verification);
clr_buildup_status clr_buildup_worker_verify_into_buffer(
    const clr_buildup_problem *problem,
    clr_build_variant_buffer *out_buffer,
    clr_buildup_verification *out_verification);
clr_buildup_status clr_buildup_verify_first(
    const clr_buildup_problem *problem,
    clr_build_variant_buffer *out_first);
clr_buildup_status clr_buildup_verify_first_with_workspace(
    const clr_buildup_problem *problem,
    clr_buildup_workspace *workspace,
    clr_build_variant_buffer *out_first);
clr_buildup_status clr_buildup_exists_with_workspace(
    const clr_buildup_problem *problem,
    clr_buildup_workspace *workspace);
clr_buildup_status clr_buildup_enumerate_variants(
    const clr_buildup_problem *problem,
    const clr_buildup_enumeration_limits *limits,
    clr_build_variant_buffer *out_variants);
clr_buildup_status clr_buildup_enumerate_variants_with_workspace(
    const clr_buildup_problem *problem,
    const clr_buildup_enumeration_limits *limits,
    clr_buildup_workspace *workspace,
    clr_build_variant_buffer *out_variants);
clr_buildup_status clr_buildup_count_variants(
    const clr_buildup_problem *problem,
    const clr_buildup_count_limits *limits,
    clr_buildup_count_report *out_report);
#endif
