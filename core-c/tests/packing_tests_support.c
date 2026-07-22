#include "packing_tests_support.h"
ClearraCacheIdentity packing_test_full_cache_identity(void) {
    ClearraCacheIdentity identity = clearra_cache_identity_zero();
    identity.board = UINT64_C(0x1234);
    identity.piece_set_profile = 1;
    identity.piece_definition_id_fingerprint = 11;
    identity.piece_area_multiset_fingerprint = 12;
    identity.rule_kick_profile = 2;
    identity.backend_mode = 3;
    identity.operation_table_version = 4;
    identity.supply_provenance = 5;
    identity.queue_pattern_id = 6;
    identity.piece_window_start = 0;
    identity.piece_window_len = 5;
    identity.goal_id = 7;
    return identity;
}
ClearraBoard64Layout packing_test_standard_two_line_layout(void) {
    ClearraBoard64Layout layout;
    if (clearra_board64_make_layout(10, 2, &layout) != CLEARRA_BOARD64_OK) {
        fprintf(stderr, "failed to create 10x2 layout\n");
        exit(1);
    }
    return layout;
}ClearraBoard64Layout packing_test_two_by_two_layout(void) {
    ClearraBoard64Layout layout;
    if (clearra_board64_make_layout(2, 2, &layout) != CLEARRA_BOARD64_OK) {
        fprintf(stderr, "failed to create 2x2 layout\n");
        exit(1);
    }
    return layout;
}ClearraPackingCandidateView packing_test_single_operation_candidate(
    ClearraBoard64Layout layout,
    uint64_t mask,
    int8_t x) {
    ClearraPackingCandidateView candidate;
    clearra_packing_candidate_view_clear(&candidate);
    candidate.final_board = mask;
    candidate.shape_mask = mask;
    candidate.placed_count = 1;
    candidate.pieces[0] = CLR_PIECE_O;
    candidate.rotations[0] = CLEARRA_ROTATION_SPAWN;
    candidate.xs[0] = x;
    candidate.ys[0] = 0;
    candidate.operation_masks[0] = mask;
    if (clearra_operation_id(CLR_PIECE_O, CLEARRA_ROTATION_SPAWN,
                             &candidate.operation_ids[0]) != CLEARRA_OPERATION_OK) {
        fprintf(stderr, "failed to create operation id\n");
        exit(1);
    }
    candidate.shape_key = clearra_packing_shape_key(layout, candidate.shape_mask);
    candidate.tiling_key = clearra_packing_tiling_key_with_piece_identity(
        layout,
        candidate.pieces,
        candidate.rotations,
        candidate.operation_masks,
        candidate.operation_deleted_row_masks,
        1);
    candidate.operation_set_key = clearra_packing_operation_set_key(&candidate);
    return candidate;
}
clr_packing_problem packing_test_short_queue_problem(
    ClearraBoard64Layout layout,
    uint64_t target_mask,
    bool exact) {
    clr_packing_problem problem = clr_packing_problem_zero();
    problem.problem_kind = CLR_PROBLEM_OPENING_PC;
    problem.max_pieces = 5;
    problem.board.width = layout.width;
    problem.board.visible_height = layout.height;
    problem.board.search_height = layout.height;
    problem.board.initial_mask = clearra_board64_empty();
    problem.board.backend_kind = CLR_BOARD_BACKEND_BOARD64;
    problem.board.cell_count = layout.cell_count;
    problem.goal_region_mask = target_mask;
    problem.required_fill_mask = target_mask;
    problem.exact_pieces = exact ? 5 : 0;
    problem.piece_window =
        clearra_piece_window_descriptor(5, exact ? 5 : 0, exact);
    uint8_t pieces[] = {CLR_PIECE_I, CLR_PIECE_O, CLR_PIECE_T};
    problem.piece_multiset_window =
        clearra_piece_multiset_window_from_pieces(pieces, 3);
    problem.piece_source = clearra_piece_source_descriptor_fixed_queue(
        1u,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE,
        3,
        CLR_PIECE_SET_STANDARD_TETROMINOES);
    problem.rule.piece_set_profile_id = CLR_PIECE_SET_STANDARD_TETROMINOES;
    problem.rule.bag_profile_id = CLR_BAG_STANDARD_7_BAG;
    problem.rule.rule_profile_id = CLR_RULE_NO_KICK;
    problem.rule.kick_profile_id = CLR_KICK_NO_KICK;
    problem.rule.spawn_profile_id = CLR_SPAWN_STANDARD_10;
    problem.goal = CLR_GOAL_CLEAR_TO_EMPTY;
    problem.count_policy = CLR_COUNT_ALL;
    problem.objective = CLR_OBJECTIVE_ALL;
    return problem;
}
void packing_test_push_raw_candidate(
    ClearraPackingCandidateBuffer *buffer,
    ClearraPackingCandidateView candidate) {
    EXPECT_STATUS(clearra_packing_candidate_buffer_push(buffer, &candidate, 0),
                  CLEARRA_PACKING_OK);
}
