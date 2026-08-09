#include "buildup_tests_support.h"
#include "../include/clr_buildup_geometry_language.h"
#include "../src/buildup/buildup_workspace.h"
#include <string.h>
void c_build_variant_exports_kick_evidence(void) {
    clr_buildup_verification verification = {0};
    clr_build_variant_buffer *buffer =
        (clr_build_variant_buffer *)malloc(sizeof(*buffer));
    EXPECT_TRUE(buffer != 0);
    clr_build_variant_buffer_clear(buffer);

    verification.accepted = 1;
    verification.variant.candidate_id = 0xabc;
    verification.variant.canonical_operation_set_id = 0xabc;
    verification.variant.operation_set_hash = 0xabc;
    verification.variant.kick_evidence = verification.kick_evidence_storage;
    verification.variant.kick_evidence_count = 1;
    verification.kick_evidence_storage[0].has_kick_evidence = 1;
    verification.kick_evidence_storage[0].from_rotation = 0;
    verification.kick_evidence_storage[0].to_rotation = 1;
    verification.kick_evidence_storage[0].rotation_request = 1;
    verification.kick_evidence_storage[0].kick_index = 2;
    verification.kick_evidence_storage[0].kick_dx = 1;
    verification.kick_evidence_storage[0].kick_dy = -1;
    verification.kick_evidence_storage[0].kick_table_id = 0x11;
    verification.kick_evidence_storage[0].kick_profile_id = 0x22;
    verification.kick_evidence_storage[0].first_success_confirmed = 1;
    verification.kick_evidence_storage[0].predecessor_x = 3;
    verification.kick_evidence_storage[0].predecessor_y = 4;
    verification.kick_evidence_storage[0].result_x = 5;
    verification.kick_evidence_storage[0].result_y = 6;

    EXPECT_BUILDUP_STATUS(
        clr_build_variant_buffer_push_verified(buffer, &verification),
        CLR_BUILDUP_OK);
    EXPECT_U64(buffer->count, 1);
    EXPECT_TRUE(buffer->variants[0].kick_evidence != verification.variant.kick_evidence);
    EXPECT_TRUE(buffer->variants[0].kick_evidence != 0);
    EXPECT_U64(buffer->variants[0].kick_evidence_count, 1);
    EXPECT_U64(buffer->variants[0].kick_evidence[0].has_kick_evidence, 1);
    EXPECT_U64(buffer->variants[0].kick_evidence[0].kick_index, 2);
    EXPECT_U64(buffer->variants[0].kick_evidence[0].result_x, 5);
    free(buffer);
}void build_variant_exports_kick_evidence(void) {
    c_build_variant_exports_kick_evidence();
}void actual_reachability_kick_evidence_reaches_build_variant(void) {
    ClearraBoard64Layout layout = buildup_test_standard_10x4_layout();
    static const ClearraKickOffset clockwise_offsets[1] = {{-2, 1}};
    ClearraReachabilityKickTable table = {
        .clockwise_offsets = clockwise_offsets,
        .clockwise_count = 1,
    };
    uint64_t board = buildup_test_cell_mask(layout, 1, 3);
    ClearraReachabilityReport report;
    EXPECT_U64(clearra_reachability_check(
                   layout,
                   board,
                   CLR_PIECE_T,
                   CLEARRA_ROTATION_RIGHT,
                   1,
                   0,
                   CLEARRA_REACHABILITY_MODE_LOCKED,
                   &table,
                   &report),
               CLEARRA_REACHABILITY_OK);
    EXPECT_TRUE(report.reachable);
    EXPECT_TRUE(report.first_success_confirmed);

    clr_buildup_operation operation = {0};
    operation.piece = CLR_PIECE_T;
    operation.rotation = CLEARRA_ROTATION_RIGHT;
    operation.x = 1;
    operation.y = 0;
    EXPECT_U64(clearra_operation_id(
                   operation.piece,
                   operation.rotation,
                   &operation.operation_id),
               CLEARRA_OPERATION_OK);
    EXPECT_U64(clearra_candidate_mask_for_piece(
                   layout,
                   operation.piece,
                   operation.rotation,
                   operation.x,
                   operation.y,
                   &operation.mask),
               CLEARRA_CANDIDATE_OK);
    uint64_t placed_board = 0;
    EXPECT_U64(clearra_board64_place(
                   layout, board, operation.mask, &placed_board),
               CLEARRA_BOARD64_OK);
    ClearraBoard64LineClearResult clear_result;
    EXPECT_U64(clearra_board64_clear_lines(
                   layout, placed_board, &clear_result),
               CLEARRA_BOARD64_OK);

    clr_buildup_problem problem = {0};
    problem.candidate_id = 17u;
    problem.canonical_operation_set_id = 23u;
    problem.rule.rule_profile_id = CLR_RULE_SRS_X;
    problem.rule.kick_profile_id = CLR_KICK_IMPORTED;
    clr_buildup_trace_step trace_step;
    clr_kick_evidence_view kick_evidence;
    ClearraBuildUpReachabilityResult reachability_result;
    clearra_buildup_reachability_result_from_report(
        &report, &reachability_result);
    clearra_buildup_trace_step_from_operation(
        &problem,
        &operation,
        0,
        operation.y,
        clear_result,
        &reachability_result,
        &trace_step,
        &kick_evidence);
    EXPECT_U64(trace_step.reachability.last_action_was_rotation, 1);
    EXPECT_U64(trace_step.reachability.rotation_evidence_complete, 1);
    EXPECT_U64(kick_evidence.has_kick_evidence, 1);
    EXPECT_U64(kick_evidence.first_success_confirmed, 1);
    EXPECT_U64(kick_evidence.from_rotation, CLEARRA_ROTATION_SPAWN);
    EXPECT_U64(kick_evidence.to_rotation, CLEARRA_ROTATION_RIGHT);
    EXPECT_U64(kick_evidence.rotation_request,
               CLEARRA_ROTATION_TRANSITION_CLOCKWISE);
    EXPECT_U64(kick_evidence.kick_index, 0);
    EXPECT_U64((uint8_t)kick_evidence.kick_dx, (uint8_t)-2);
    EXPECT_U64(kick_evidence.kick_dy, 1);
    EXPECT_U64(kick_evidence.predecessor_x, 2);
    EXPECT_U64(kick_evidence.result_x, 1);

    ClearraBuildUpSearchContext context = {0};
    context.capture_trace = 1u;
    context.current_trace_steps[0] = trace_step;
    context.current_kick_evidence[0] = kick_evidence;
    clearra_buildup_capture_success_path(&context, 1u);
    ClearraBuildUpState state = {0};
    state.placed_pieces = 1u;
    clr_build_variant_view variant;
    clearra_build_variant_from_state(&problem, &state, &variant);
    clearra_buildup_attach_success_trace(&context, &variant);
    EXPECT_U64(variant.kick_evidence_count, 1);
    EXPECT_U64(variant.trace_steps[0].kick_evidence_index, 0);
    EXPECT_TRUE(
        (variant.trace_completeness_flags &
         CLR_BUILDUP_TRACE_COMPLETENESS_KICK_EVIDENCE_MISSING) == 0u);

    clr_buildup_verification verification = {0};
    verification.accepted = 1u;
    verification.variant = variant;
    clr_build_variant_buffer *buffer =
        (clr_build_variant_buffer *)malloc(sizeof(*buffer));
    EXPECT_TRUE(buffer != 0);
    clr_build_variant_buffer_clear(buffer);
    EXPECT_BUILDUP_STATUS(
        clr_build_variant_buffer_push_verified(buffer, &verification),
        CLR_BUILDUP_OK);
    EXPECT_U64(buffer->variants[0].kick_evidence_count, 1);
    EXPECT_U64(buffer->variants[0].kick_evidence[0].predecessor_x, 2);
    EXPECT_U64(buffer->variants[0].kick_evidence[0].result_x, 1);
    free(buffer);
}void kick_evidence_buffer_reports_capacity_exhausted(void) {
    clr_buildup_verification verification = {0};
    clr_build_variant_buffer *buffer =
        (clr_build_variant_buffer *)malloc(sizeof(*buffer));
    EXPECT_TRUE(buffer != 0);
    clr_build_variant_buffer_clear(buffer);

    verification.accepted = 1;
    verification.variant.kick_evidence = verification.kick_evidence_storage;
    verification.variant.kick_evidence_count =
        CLR_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT + 1u;

    EXPECT_BUILDUP_STATUS(
        clr_build_variant_buffer_push_verified(buffer, &verification),
        CLR_KICK_EVIDENCE_BUFFER_EXHAUSTED);
    EXPECT_U64(buffer->count, 0);
    free(buffer);
}
void kick_evidence_buffer_budget_rejects_exhaustion(void) {
    kick_evidence_buffer_reports_capacity_exhausted();
}
void kick_evidence_missing_sets_trace_completeness_flag(void) {
    clr_buildup_trace_step step = {0};
    step.reachability.last_action_was_rotation = 1u;
    step.reachability.rotation_evidence_complete = 0u;
    clr_build_variant_view variant = {0};
    clearra_buildup_apply_kick_trace_completeness(&step, 1u, &variant);
    EXPECT_TRUE(
        (variant.trace_completeness_flags &
         CLR_BUILDUP_TRACE_COMPLETENESS_KICK_EVIDENCE_MISSING) != 0u);
}void kick_profile_without_rotation_evidence_remains_complete(void) {
    clr_buildup_trace_step step = {0};
    step.reachability.reachable = 1u;
    step.reachability.last_action_was_rotation = 0u;
    step.reachability.rotation_evidence_complete = 1u;
    clr_build_variant_view variant = {0};
    variant.trace_completeness_flags =
        CLR_BUILDUP_TRACE_COMPLETENESS_KICK_EVIDENCE_MISSING;
    clearra_buildup_apply_kick_trace_completeness(&step, 1u, &variant);
    EXPECT_TRUE(
        (variant.trace_completeness_flags &
         CLR_BUILDUP_TRACE_COMPLETENESS_KICK_EVIDENCE_MISSING) == 0u);
}void same_operation_set_different_order_produces_distinct_trace_identity(void) {
    clr_buildup_trace_step first_order[2] = {0};
    clr_buildup_trace_step second_order[2] = {0};

    first_order[0].operation_id = 11u;
    first_order[0].operation_index = 0u;
    first_order[0].kick_evidence_index = UINT8_MAX;
    first_order[1].operation_id = 22u;
    first_order[1].operation_index = 1u;
    first_order[1].kick_evidence_index = UINT8_MAX;

    second_order[0] = first_order[1];
    second_order[1] = first_order[0];

    uint64_t first_identity =
        clearra_buildup_trace_identity(first_order, 2u);
    uint64_t second_identity =
        clearra_buildup_trace_identity(second_order, 2u);

    EXPECT_TRUE(first_identity != 0u);
    EXPECT_TRUE(second_identity != 0u);
    EXPECT_TRUE(first_identity != second_identity);
}

void prepared_geometry_language_v2_exports_semantic_nodes_and_edges(void) {
    clr_buildup_problem problem = buildup_test_two_operation_gap_fill_problem();
    clr_buildup_workspace *workspace = clr_buildup_workspace_create();
    EXPECT_TRUE(workspace != 0);

    clr_buildup_geometry_language_report_v2 prepared = {0};
    EXPECT_BUILDUP_STATUS(
        clr_buildup_prepare_geometry_language_v2_with_workspace(
            &problem,
            workspace,
            CLR_BUILDUP_GEOMETRY_TRANSITION_GEOMETRY_ONLY,
            &prepared),
        CLR_BUILDUP_OK);
    EXPECT_U64(prepared.complete, 1u);
    EXPECT_U64(prepared.format_version, 2u);
    EXPECT_U64(
        prepared.transition_mode,
        CLR_BUILDUP_GEOMETRY_TRANSITION_GEOMETRY_ONLY);
    EXPECT_TRUE(prepared.snapshot_id != 0u);
    EXPECT_TRUE(prepared.node_count >= 2u);
    EXPECT_TRUE(prepared.edge_count >= 1u);

    clr_buildup_geometry_language_node_v2 *nodes =
        (clr_buildup_geometry_language_node_v2 *)calloc(
            prepared.node_count, sizeof(*nodes));
    clr_buildup_geometry_language_edge_v2 *edges =
        (clr_buildup_geometry_language_edge_v2 *)calloc(
            prepared.edge_count, sizeof(*edges));
    EXPECT_TRUE(nodes != 0);
    EXPECT_TRUE(edges != 0);

    clr_buildup_geometry_language_report_v2 copied = {0};
    EXPECT_BUILDUP_STATUS(
        clr_buildup_copy_prepared_geometry_language_v2(
            workspace,
            nodes,
            prepared.node_count,
            edges,
            prepared.edge_count,
            &copied),
        CLR_BUILDUP_OK);
    EXPECT_TRUE(memcmp(&prepared, &copied, sizeof(prepared)) == 0);

    const clr_buildup_geometry_language_node_v2 *root =
        &nodes[copied.root_node_index];
    EXPECT_U64(root->board_mask, problem.initial_board.initial_mask);
    EXPECT_U64(root->reachability_relevant_state, root->board_mask);
    EXPECT_U64(root->remaining_operations, 3u);
    EXPECT_U64(root->deleted_row_mask, 0u);
    EXPECT_U64(root->deleted_count, 0u);
    EXPECT_U64(root->cleared_lines, 0u);
    EXPECT_U64(root->depth, 0u);
    EXPECT_TRUE(root->edge_count != 0u);

    bool saw_two_line_clear = false;
    for (uint32_t index = 0u; index < copied.edge_count; ++index) {
        const clr_buildup_geometry_language_edge_v2 *edge = &edges[index];
        EXPECT_TRUE(edge->target_mask != 0u);
        EXPECT_TRUE(edge->child_node_index < copied.node_count);
        EXPECT_TRUE(edge->operation_index < 2u);
        EXPECT_U64(edge->piece, CLR_PIECE_O);
        EXPECT_U64(edge->rotation, CLEARRA_ROTATION_SPAWN);
        EXPECT_U64(
            edge->x,
            problem.operation_set.operations[edge->operation_index].x);
        if (edge->cleared_lines == 2u) {
            EXPECT_U64(edge->cleared_row_mask, 3u);
            saw_two_line_clear = true;
        }
    }
    EXPECT_TRUE(saw_two_line_clear);

    clr_buildup_geometry_language_report_v2 copied_again = {0};
    EXPECT_BUILDUP_STATUS(
        clr_buildup_copy_prepared_geometry_language_v2(
            workspace,
            nodes,
            prepared.node_count,
            edges,
            prepared.edge_count,
            &copied_again),
        CLR_BUILDUP_OK);
    EXPECT_TRUE(memcmp(&prepared, &copied_again, sizeof(prepared)) == 0);

    free(edges);
    free(nodes);
    clr_buildup_workspace_release(workspace);
}

void geometry_only_transition_does_not_share_reachability_cache_entries(void) {
    ClearraBoard64Layout layout = buildup_test_standard_10x4_layout();
    uint8_t columns[1] = {4};
    clr_packing_problem packing = buildup_test_buildup_packing_problem(4, 1, 0);
    packing.rule.rule_profile_id = CLR_RULE_SRS;
    packing.rule.kick_profile_id = CLR_KICK_SRS_90;
    const uint8_t pieces[1] = {CLR_PIECE_O};
    buildup_test_set_packing_pieces(
        &packing,
        pieces,
        1,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);
    packing.board.initial_mask =
        buildup_test_cell_mask(layout, 3, 0) |
        buildup_test_cell_mask(layout, 3, 1) |
        buildup_test_cell_mask(layout, 6, 0) |
        buildup_test_cell_mask(layout, 6, 1) |
        buildup_test_cell_mask(layout, 4, 2) |
        buildup_test_cell_mask(layout, 5, 2);
    packing.board.cell_count = (uint32_t)packing.board.width *
                               (uint32_t)packing.board.search_height;
    clr_buildup_problem problem = buildup_test_build_problem_from_candidate(
        packing,
        buildup_test_o_candidate_for_columns(layout, columns, 1));

    clr_buildup_workspace *workspace = clr_buildup_workspace_create();
    EXPECT_TRUE(workspace != 0);
    EXPECT_BUILDUP_STATUS(
        clearra_buildup_workspace_prepare(workspace, &problem),
        CLR_BUILDUP_OK);
    ClearraBuildUpSearchContext context = {0};
    EXPECT_BUILDUP_STATUS(
        clearra_buildup_search_context_init_with_reachability(
            &problem, &workspace->compiled_rule, &context),
        CLR_BUILDUP_OK);
    context.operation_variant_cache = &workspace->operation_variant_cache;
    context.reachability_cache = &workspace->reachability_cache;
    context.reachable_lock_cache = &workspace->reachable_lock_cache;
    context.reachability_frontier = &workspace->reachability_frontier;
    context.geometry_transition_cache = &workspace->geometry_transition_cache;
    context.reachability_trace_mode = CLEARRA_REACHABILITY_TRACE_NONE;

    ClearraBuildUpState state = clearra_buildup_state_initial(&problem);
    ClearraBuildUpState next_state;
    clr_buildup_trace_step trace_step;
    clr_kick_evidence_view kick_evidence;
    ClearraBuildUpGeometryTransitionView geometry;
    const clr_buildup_operation *operation = &problem.operation_set.operations[0];

    context.geometry_transition_mode =
        CLR_BUILDUP_GEOMETRY_TRANSITION_REACHABLE;
    EXPECT_BUILDUP_STATUS(
        clearra_buildup_search_try_operation_with_geometry(
            &context,
            state,
            (ClearraBuildUpQueueHold){0},
            operation,
            0u,
            &next_state,
            &trace_step,
            &kick_evidence,
            &geometry),
        CLR_BUILDUP_REACHABILITY_IMPOSSIBLE);

    context.geometry_transition_mode =
        CLR_BUILDUP_GEOMETRY_TRANSITION_GEOMETRY_ONLY;
    EXPECT_BUILDUP_STATUS(
        clearra_buildup_search_try_operation_with_geometry(
            &context,
            state,
            (ClearraBuildUpQueueHold){0},
            operation,
            0u,
            &next_state,
            &trace_step,
            &kick_evidence,
            &geometry),
        CLR_BUILDUP_OK);
    EXPECT_TRUE(geometry.target_mask != 0u);

    context.geometry_transition_mode =
        CLR_BUILDUP_GEOMETRY_TRANSITION_REACHABLE;
    EXPECT_BUILDUP_STATUS(
        clearra_buildup_search_try_operation_with_geometry(
            &context,
            state,
            (ClearraBuildUpQueueHold){0},
            operation,
            0u,
            &next_state,
            &trace_step,
            &kick_evidence,
            &geometry),
        CLR_BUILDUP_REACHABILITY_IMPOSSIBLE);

    clr_buildup_workspace_release(workspace);
}
