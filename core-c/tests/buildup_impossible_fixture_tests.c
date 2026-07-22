#include "buildup_tests_support.h"
void packing_possible_but_queue_order_impossible_fixture(void) {
    ClearraBoard64Layout layout = buildup_test_standard_10x2_layout();
    uint8_t columns[1] = {0};
    clr_packing_problem packing = buildup_test_buildup_packing_problem(2, 1, 1);
    const uint8_t pieces[2] = {CLR_PIECE_I, CLR_PIECE_T};
    buildup_test_set_packing_pieces(
        &packing,
        pieces,
        2,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);

    clr_buildup_problem problem =
        buildup_test_build_problem_from_candidate(packing, buildup_test_o_candidate_for_columns(layout, columns, 1));
    clr_buildup_verification verification;

    EXPECT_BUILDUP_STATUS(clr_buildup_worker_verify(&problem, &verification),
                          CLR_BUILDUP_QUEUE_ORDER_IMPOSSIBLE);
    EXPECT_U64(verification.accepted, 0);
    EXPECT_U64(verification.rejected_step, 0);
}
void packing_possible_but_hold_disabled_impossible_fixture(void) {
    ClearraBoard64Layout layout = buildup_test_standard_10x2_layout();
    uint8_t columns[1] = {0};
    clr_packing_problem packing = buildup_test_buildup_packing_problem(2, 1, 0);
    const uint8_t pieces[2] = {CLR_PIECE_I, CLR_PIECE_O};
    buildup_test_set_packing_pieces(
        &packing,
        pieces,
        2,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);

    clr_buildup_problem problem =
        buildup_test_build_problem_from_candidate(packing, buildup_test_o_candidate_for_columns(layout, columns, 1));
    clr_buildup_verification verification;

    EXPECT_BUILDUP_STATUS(clr_buildup_worker_verify(&problem, &verification),
                          CLR_BUILDUP_HOLD_DISABLED_IMPOSSIBLE);
    EXPECT_U64(verification.accepted, 0);
}
void packing_possible_but_line_clear_y_adjustment_impossible_fixture(void) {
    ClearraBoard64Layout layout = buildup_test_standard_10x2_layout();
    uint8_t columns[2] = {8, 0};
    clr_packing_problem packing = buildup_test_buildup_packing_problem(2, 2, 0);
    for (uint8_t x = 0; x < 8; x++) {
        packing.board.initial_mask |= buildup_test_cell_mask(layout, x, 0);
    }
    packing.board.cell_count = (uint32_t)packing.board.width *
                               (uint32_t)packing.board.search_height;
    const uint8_t pieces[2] = {CLR_PIECE_O, CLR_PIECE_O};
    buildup_test_set_packing_pieces(
        &packing,
        pieces,
        2,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);

    clr_buildup_problem problem =
        buildup_test_build_problem_from_candidate(packing, buildup_test_o_candidate_for_columns(layout, columns, 2));
    clr_buildup_verification verification;

    EXPECT_BUILDUP_STATUS(clr_buildup_worker_verify(&problem, &verification),
                          CLR_BUILDUP_Y_ADJUSTMENT_IMPOSSIBLE);
    EXPECT_U64(verification.accepted, 0);
    EXPECT_U64(verification.rejected_step, 1);
}
void packing_possible_but_srs_reachability_impossible_fixture(void) {
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
        buildup_test_cell_mask(layout, 3, 0) | buildup_test_cell_mask(layout, 3, 1) |
        buildup_test_cell_mask(layout, 6, 0) | buildup_test_cell_mask(layout, 6, 1) |
        buildup_test_cell_mask(layout, 4, 2) | buildup_test_cell_mask(layout, 5, 2);
    packing.board.cell_count = (uint32_t)packing.board.width *
                               (uint32_t)packing.board.search_height;

    clr_buildup_problem problem =
        buildup_test_build_problem_from_candidate(packing, buildup_test_o_candidate_for_columns(layout, columns, 1));
    clr_buildup_verification verification;

    EXPECT_BUILDUP_STATUS(clr_buildup_worker_verify(&problem, &verification),
                          CLR_BUILDUP_REACHABILITY_IMPOSSIBLE);
    EXPECT_U64(verification.accepted, 0);
}
void buildup_reachability_bridge_uses_no_kick_srs_srs_plus_and_imported_tables(void) {
    buildup_test_assert_buildup_reachability_bridge_uses_rule_kick_table(
        buildup_test_rule_descriptor(CLR_RULE_NO_KICK, CLR_KICK_NO_KICK),
        CLR_PIECE_O,
        CLEARRA_ROTATION_SPAWN,
        CLR_KICK_NO_KICK,
        false);
    buildup_test_assert_buildup_reachability_bridge_uses_rule_kick_table(
        buildup_test_rule_descriptor(CLR_RULE_SRS, CLR_KICK_SRS_90),
        CLR_PIECE_T,
        CLEARRA_ROTATION_RIGHT,
        CLR_KICK_SRS_90,
        false);
    buildup_test_assert_buildup_reachability_bridge_uses_rule_kick_table(
        buildup_test_rule_descriptor(CLR_RULE_SRS_PLUS, CLR_KICK_SRS_PLUS_180),
        CLR_PIECE_T,
        CLEARRA_ROTATION_REVERSE,
        CLR_KICK_SRS_PLUS_180,
        true);
    buildup_test_assert_buildup_reachability_bridge_uses_rule_kick_table(
        buildup_test_imported_verified_kick_descriptor(),
        CLR_PIECE_T,
        CLEARRA_ROTATION_RIGHT,
        CLR_KICK_IMPORTED,
        true);
}
void bag_aligned_pattern_duplicate_is_not_a_build_variant_fixture(void) {
    ClearraBoard64Layout layout = buildup_test_standard_10x2_layout();
    uint8_t columns[1] = {0};
    clr_packing_problem packing = buildup_test_buildup_packing_problem(2, 2, 0);
    packing.exact_pieces = 0u;
    packing.piece_window.exact_pieces = 0u;
    packing.piece_window.has_exact_pieces = 0u;
    const uint8_t pieces[2] = {CLR_PIECE_O, CLR_PIECE_O};
    buildup_test_set_packing_pieces(
        &packing,
        pieces,
        2,
        CLR_PIECE_SOURCE_BAG_UNIVERSE,
        CLR_SUPPLY_PROVENANCE_BAG_ALIGNED_PATTERN);

    clr_buildup_problem problem =
        buildup_test_build_problem_from_candidate(packing, buildup_test_o_candidate_for_columns(layout, columns, 1));
    clr_buildup_verification verification;

    EXPECT_BUILDUP_STATUS(clr_buildup_worker_verify(&problem, &verification),
                          CLR_BUILDUP_BAG_PATTERN_IMPOSSIBLE);
    EXPECT_U64(verification.accepted, 0);
}
void representative_order_hint_is_priority_not_single_path_fixture(void) {
    ClearraBoard64Layout layout = buildup_test_standard_10x2_layout();
    clr_packing_problem packing = buildup_test_buildup_packing_problem(2, 2, 0);
    const uint8_t pieces[2] = {CLR_PIECE_T, CLR_PIECE_O};
    buildup_test_set_packing_pieces(
        &packing,
        pieces,
        2,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);
    packing.board.initial_mask = UINT64_C(0x000f83e5);
    packing.board.cell_count = (uint32_t)packing.board.width *
                               (uint32_t)packing.board.search_height;

    clr_buildup_problem problem = buildup_test_build_problem_from_candidate(
        packing, buildup_test_representative_order_hint_is_not_solution_order_candidate(layout));
    clr_buildup_verification verification;

    EXPECT_U64(problem.operation_set.representative_order_hint[0], 0);
    EXPECT_U64(problem.operation_set.operations[0].piece, CLR_PIECE_O);
    EXPECT_U64(problem.packing.piece_multiset_window.counts[CLR_PIECE_T], 1);
    EXPECT_U64(problem.packing.piece_multiset_window.counts[CLR_PIECE_O], 1);
    EXPECT_BUILDUP_STATUS(clr_buildup_worker_verify(&problem, &verification),
                          CLR_BUILDUP_OK);
    EXPECT_U64(verification.accepted, 1);
    EXPECT_U64(verification.variant.final_board, 0);
    EXPECT_U64(verification.variant.queue_cursor, 2);
    EXPECT_U64(verification.variant.cleared_lines, 2);
}void buildup_exports_actual_success_operation_order(void) {
    ClearraBoard64Layout layout = buildup_test_standard_10x2_layout();
    clr_packing_problem packing = buildup_test_buildup_packing_problem(2, 2, 0);
    const uint8_t pieces[2] = {CLR_PIECE_T, CLR_PIECE_O};
    buildup_test_set_packing_pieces(
        &packing, pieces, 2, CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);
    packing.board.initial_mask = UINT64_C(0x000f83e5);
    packing.board.cell_count = (uint32_t)packing.board.width *
                               (uint32_t)packing.board.search_height;

    ClearraPackingCandidateView candidate =
        buildup_test_representative_order_hint_is_not_solution_order_candidate(
            layout);
    clr_buildup_problem problem =
        buildup_test_build_problem_from_candidate(packing, candidate);
    clr_buildup_verification verification;

    EXPECT_BUILDUP_STATUS(
        clr_buildup_worker_verify(&problem, &verification), CLR_BUILDUP_OK);
    EXPECT_U64(verification.variant.operation_order_count, 2);
    EXPECT_U64(verification.variant.trace_step_count, 2);
    EXPECT_TRUE(verification.variant.operation_order_ids != 0);
    EXPECT_TRUE(verification.variant.trace_steps != 0);
    EXPECT_U64(verification.variant.operation_order_ids[0],
               candidate.operation_ids[1]);
    EXPECT_U64(verification.variant.operation_order_ids[1],
               candidate.operation_ids[0]);
    EXPECT_U64(verification.variant.trace_steps[0].operation_index, 1);
    EXPECT_U64(verification.variant.trace_steps[1].operation_index, 0);
    EXPECT_U64(verification.variant.trace_steps[0].piece, CLR_PIECE_T);
    EXPECT_U64(verification.variant.trace_steps[1].piece, CLR_PIECE_O);
    EXPECT_TRUE(verification.variant.trace_identity != 0u);
}
void valid_packing_and_valid_buildup_becomes_build_variant_fixture(void) {
    ClearraBoard64Layout layout = buildup_test_standard_10x2_layout();
    uint8_t columns[5] = {0, 2, 4, 6, 8};
    clr_packing_problem packing = buildup_test_buildup_packing_problem(2, 5, 0);
    const uint8_t pieces[5] = {
        CLR_PIECE_O,
        CLR_PIECE_O,
        CLR_PIECE_O,
        CLR_PIECE_O,
        CLR_PIECE_O,
    };
    buildup_test_set_packing_pieces(
        &packing,
        pieces,
        5,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);

    clr_buildup_problem problem =
        buildup_test_build_problem_from_candidate(packing, buildup_test_o_candidate_for_columns(layout, columns, 5));
    clr_buildup_verification verification;
    clr_build_variant_buffer *buffer =
        (clr_build_variant_buffer *)malloc(sizeof(*buffer));
    EXPECT_TRUE(buffer != 0);
    clr_build_variant_buffer_clear(buffer);

    EXPECT_BUILDUP_STATUS(
        clr_buildup_worker_verify_into_buffer(&problem, buffer, &verification),
        CLR_BUILDUP_OK);
    EXPECT_U64(verification.accepted, 1);
    EXPECT_U64(verification.variant.final_board, 0);
    EXPECT_U64(verification.variant.placed_count, 5);
    EXPECT_U64(verification.variant.queue_cursor, 5);
    EXPECT_U64(verification.variant.cleared_lines, 2);
    EXPECT_U64(buffer->count, 1);
    free(buffer);
}void build_up_bfs_matches_fixture(void) {
    valid_packing_and_valid_buildup_becomes_build_variant_fixture();
}
