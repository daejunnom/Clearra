#include "buildup_tests_support.h"
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
