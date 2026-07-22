#include "buildup_tests_support.h"
void buildup_rejects_queue_order_mismatch(void) {
    uint64_t first_cursor_key =
        clearra_buildup_memo_key(buildup_test_full_cache_identity(), UINT64_C(0x77), 0, 1, 0);
    uint64_t second_cursor_key =
        clearra_buildup_memo_key(buildup_test_full_cache_identity(), UINT64_C(0x77), 0, 2, 0);

    EXPECT_TRUE(first_cursor_key != second_cursor_key);
}
void buildup_event_preserves_board_before_and_after(void) {
    ClearraBuildUpState state = {0};
    ClearraBuildUpEvent event = {0};

    state.hold_automaton_state.cursor = 1;
    state.placed_pieces = 1;
    event.kind = CLEARRA_BUILDUP_EVENT_PLACEMENT;
    event.board_before = UINT64_C(0x000f);
    event.board_after = UINT64_C(0x00ff);
    event.cleared_lines = 1;

    EXPECT_U64(state.hold_automaton_state.cursor, 1);
    EXPECT_U64(event.kind, CLEARRA_BUILDUP_EVENT_PLACEMENT);
    EXPECT_U64(event.board_before, UINT64_C(0x000f));
    EXPECT_U64(event.board_after, UINT64_C(0x00ff));
    EXPECT_U64(event.cleared_lines, 1);
}
void packing_candidate_converts_to_buildup_problem(void) {
    clr_packing_problem packing = buildup_test_valid_packing_problem();
    ClearraPackingCandidateView candidate = buildup_test_two_operation_candidate();
    clr_buildup_problem problem;

    EXPECT_U64(clearra_buildup_problem_from_packing_candidate(
                   &packing, &candidate, 42, &problem),
               CLEARRA_PACKING_OK);
    EXPECT_TRUE(clr_buildup_problem_is_valid(&problem));
    EXPECT_U64(problem.initial_board.initial_mask, UINT64_C(0x30));
    EXPECT_U64(problem.operation_set.operation_count, 2);
    EXPECT_U64(problem.operation_set.representative_order_hint[0], 0);
    EXPECT_U64(problem.operation_set.representative_order_hint[1], 1);
    EXPECT_U64(problem.operation_set.operations[0].piece, CLR_PIECE_O);
    EXPECT_U64(problem.operation_set.operations[1].piece, CLR_PIECE_I);
    EXPECT_U64(problem.packing.piece_multiset_window.counts[CLR_PIECE_O], 1);
    EXPECT_U64(problem.packing.piece_multiset_window.counts[CLR_PIECE_I], 1);
    EXPECT_U64(problem.piece_source.piece_source_id,
               problem.packing.piece_source.piece_source_id);
    EXPECT_U64(problem.initial_hold_automaton.piece_source_id,
               problem.piece_source.piece_source_id);
    EXPECT_U64(problem.rule.kick_profile_id, CLR_KICK_SRS_PLUS_180);
    EXPECT_U64(problem.line_clear_policy, CLR_LINE_CLEAR_POLICY_STANDARD);
    EXPECT_U64(problem.piece_window.max_pieces, 5);
    EXPECT_U64(problem.goal, CLR_GOAL_CLEAR_TO_EMPTY);
    EXPECT_U64(problem.coverage_pattern_id, 42);
    EXPECT_U64(problem.piece_source_pattern_id, 42);
    EXPECT_U64(problem.packing.piece_source_pattern_id, 42);
}
void buildup_state_starts_from_problem_initial_board_hold_and_cursor(void) {
    clr_packing_problem packing = buildup_test_valid_packing_problem();
    ClearraPackingCandidateView candidate = buildup_test_two_operation_candidate();
    clr_buildup_problem problem;
    EXPECT_U64(clearra_buildup_problem_from_packing_candidate(
                   &packing, &candidate, 7, &problem),
               CLEARRA_PACKING_OK);

    ClearraBuildUpState state = clearra_buildup_state_initial(&problem);
    EXPECT_U64(state.board_mask, UINT64_C(0x30));
    EXPECT_U64(state.hold_automaton_state.cursor, 0);
    EXPECT_U64(state.hold_automaton_state.hold_empty, 1);
    EXPECT_U64(state.placed_pieces, 0);
}static clr_buildup_bfs_state buildup_test_bfs_state(void) {
    clr_buildup_bfs_state state = {0};
    EXPECT_BUILDUP_STATUS(
        clearra_buildup_remaining_ops_bitset_for_count(3u, &state.remaining_ops_bitset),
        CLR_BUILDUP_OK);
    state.current_board_mask = UINT64_C(0x30);
    state.deleted_line_state.deleted_row_mask = 0u;
    state.deleted_line_state.deleted_count = 0u;
    state.hold_automaton_state.piece_source_id = 11u;
    state.hold_automaton_state.cursor = 2u;
    state.hold_automaton_state.bag_epoch = 1u;
    state.hold_automaton_state.bag_remainder_key = UINT64_C(0xabc);
    state.hold_automaton_state.provenance_id = 77u;
    state.hold_automaton_state.hold_piece = CLR_PIECE_T;
    state.hold_automaton_state.hold_empty = 0u;
    state.piece_source_cursor = state.hold_automaton_state.cursor;
    state.reachability_relevant_state = UINT64_C(0x123456);
    state.cleared_lines = 0u;
    return state;
}static uint64_t buildup_test_bfs_memo_hash(clr_buildup_bfs_state state) {
    clr_buildup_memo_key key = clearra_buildup_memo_key_from_bfs_state(
        buildup_test_full_cache_identity(), &state);
    return clearra_buildup_memo_key_hash(&key);
}void buildup_state_contains_deleted_line_state(void) {
    clr_buildup_bfs_state state = buildup_test_bfs_state();

    EXPECT_TRUE(clearra_buildup_bfs_state_has_deleted_line_state(&state));
    state.deleted_line_state.deleted_row_mask = UINT16_C(1) << 2;
    state.deleted_line_state.deleted_count = 1u;
    EXPECT_U64(state.deleted_line_state.deleted_row_mask, UINT16_C(1) << 2);
    EXPECT_U64(state.deleted_line_state.deleted_count, 1u);
}void buildup_state_contains_hold_automaton_state(void) {
    clr_buildup_bfs_state state = buildup_test_bfs_state();

    EXPECT_TRUE(clearra_buildup_bfs_state_has_hold_automaton_state(&state));
    EXPECT_U64(state.hold_automaton_state.piece_source_id, 11u);
    EXPECT_U64(state.hold_automaton_state.bag_epoch, 1u);
    EXPECT_U64(state.hold_automaton_state.bag_remainder_key, UINT64_C(0xabc));
}void buildup_memo_key_differs_by_deleted_line_state(void) {
    clr_buildup_bfs_state left = buildup_test_bfs_state();
    clr_buildup_bfs_state right = left;
    right.deleted_line_state.deleted_row_mask = UINT16_C(1) << 3;
    right.deleted_line_state.deleted_count = 1u;

    EXPECT_TRUE(buildup_test_bfs_memo_hash(left) !=
                buildup_test_bfs_memo_hash(right));
}void buildup_memo_key_differs_by_bag_epoch(void) {
    clr_buildup_bfs_state left = buildup_test_bfs_state();
    clr_buildup_bfs_state right = left;
    right.hold_automaton_state.bag_epoch++;

    EXPECT_TRUE(buildup_test_bfs_memo_hash(left) !=
                buildup_test_bfs_memo_hash(right));
}void buildup_memo_key_differs_by_bag_remainder_key(void) {
    clr_buildup_bfs_state left = buildup_test_bfs_state();
    clr_buildup_bfs_state right = left;
    right.hold_automaton_state.bag_remainder_key ^= UINT64_C(0x40);

    EXPECT_TRUE(buildup_test_bfs_memo_hash(left) !=
                buildup_test_bfs_memo_hash(right));
}void buildup_memo_key_differs_by_reachability_state(void) {
    clr_buildup_bfs_state left = buildup_test_bfs_state();
    clr_buildup_bfs_state right = left;
    right.reachability_relevant_state ^= UINT64_C(0x8000);

    EXPECT_TRUE(buildup_test_bfs_memo_hash(left) !=
                buildup_test_bfs_memo_hash(right));
}void mvp1_buildup_15_operation_fast_path_unchanged(void) {
    EXPECT_U64(clearra_buildup_mvp1_max_operations(), 15);
    EXPECT_U64(CLR_BUILDUP_MAX_OPERATIONS, 15);
    EXPECT_BUILDUP_STATUS(
        clearra_buildup_operation_set_runtime_status(CLR_BUILDUP_MAX_OPERATIONS),
        CLR_BUILDUP_OK);
}void operation_count_above_runtime_limit_is_unsupported(void) {
    EXPECT_BUILDUP_STATUS(clearra_buildup_operation_set_runtime_status(16u),
                          CLR_BUILDUP_UNSUPPORTED_RUNTIME_SCOPE);
}void board128_buildup_guard_reports_unsupported(void) {
    clr_board_descriptor board = {0};
    board.width = 10;
    board.visible_height = 8;
    board.search_height = 8;
    board.backend_kind = CLR_BOARD_BACKEND_BOARD128;
    board.cell_count = 80;

    EXPECT_BUILDUP_STATUS(clearra_buildup_runtime_status_for_board(&board),
                          CLR_BUILDUP_UNSUPPORTED_RUNTIME_SCOPE);
}void unsupported_buildup_scope_does_not_claim_solution(void) {
    clr_packing_problem packing = buildup_test_valid_packing_problem();
    packing.board.backend_kind = CLR_BOARD_BACKEND_BOARD128;
    packing.board.cell_count = 80;
    packing.board.visible_height = 8;
    packing.board.search_height = 8;

    clr_buildup_problem problem = clr_buildup_problem_from_packing(packing);
    problem.initial_board = packing.board;
    problem.operation_set.operation_count = 1;
    clr_buildup_verification verification;

    EXPECT_BUILDUP_STATUS(clr_buildup_worker_verify(&problem, &verification),
                          CLR_BUILDUP_UNSUPPORTED_RUNTIME_SCOPE);
    EXPECT_U64(verification.accepted, 0);
    EXPECT_U64(verification.reject_reason, CLR_BUILDUP_UNSUPPORTED_RUNTIME_SCOPE);
}void y_adjustment_uses_deleted_row_mask_not_deleted_count(void) {
    ClearraBoard64Layout layout = buildup_test_standard_10x4_layout();
    ClearraBuildUpState state = {0};
    clr_buildup_operation operation = {0};
    uint64_t adjusted_mask = 0;
    int8_t adjusted_y = -1;

    state.line_clear_state.deleted_row_mask = UINT16_C(1) << 3;
    state.line_clear_state.deleted_count = 1;
    state.cleared_lines = 1;
    operation.piece = CLR_PIECE_O;
    operation.rotation = CLEARRA_ROTATION_SPAWN;
    operation.x = 0;
    operation.y = 0;
    operation.mask = buildup_test_o_mask_at(layout, 0, 0);
    operation.required_deleted_row_mask = state.line_clear_state.deleted_row_mask;

    EXPECT_BUILDUP_STATUS(clearra_buildup_adjust_operation_for_line_clears(
                              layout, state, &operation, &adjusted_mask, &adjusted_y),
                          CLR_BUILDUP_OK);
    EXPECT_U64(adjusted_mask, operation.mask);
    EXPECT_U64(adjusted_y, 0);
}

void geometry_variant_domain_survives_intermediate_line_clear(void) {
    clr_buildup_problem problem = {0};
    ClearraBuildUpState state = {0};

    problem.operation_set.operation_count = 1u;
    problem.operation_set.geometry_variant_domains = UINT16_C(1);
    problem.operation_set.operations[0].required_deleted_row_mask = 0u;
    state.line_clear_state.deleted_row_mask = UINT16_C(1);
    state.line_clear_state.deleted_count = 1u;
    state.cleared_lines = 1u;

    EXPECT_TRUE(clearra_buildup_operation_domain_may_match_clear_state(
        &problem, &state, 0u));

    problem.operation_set.geometry_variant_domains = 0u;
    EXPECT_TRUE(!clearra_buildup_operation_domain_may_match_clear_state(
        &problem, &state, 0u));
}

void buildup_worker_uses_search_height_when_visible_height_differs(void) {
    ClearraBoard64Layout search_layout;
    if (clearra_board64_make_layout(2, 2, &search_layout) != CLEARRA_BOARD64_OK) {
        fprintf(stderr, "failed to create 2x2 search layout\n");
        exit(1);
    }

    clr_packing_problem packing = clr_packing_problem_zero();
    packing.problem_kind = CLR_PROBLEM_SCENARIO_PC;
    packing.max_pieces = 1;
    buildup_test_set_board_descriptor(&packing.board, 2, 1, 2, clearra_board64_empty());
    packing.goal_region_mask = buildup_test_low_mask_for_cells(4);
    packing.required_fill_mask = packing.goal_region_mask;
    packing.exact_pieces = 1;
    packing.piece_window.max_pieces = 1;
    packing.piece_window.exact_pieces = 1;
    packing.piece_window.has_exact_pieces = 1;
    const uint8_t pieces[1] = {CLR_PIECE_O};
    packing.piece_multiset_window =
        clearra_piece_multiset_window_from_pieces(pieces, 1);
    packing.piece_source = clearra_piece_source_descriptor_fixed_queue(
        1u,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE,
        1,
        CLR_PIECE_SET_STANDARD_TETROMINOES);
    buildup_test_set_piece_source_pattern_cache(
        &packing,
        pieces,
        1,
        1u,
        CLR_SUPPLY_TRUNCATION_NONE);
    packing.rule.piece_set_profile_id = CLR_PIECE_SET_STANDARD_TETROMINOES;
    packing.rule.bag_profile_id = CLR_BAG_STANDARD_7_BAG;
    packing.rule.rule_profile_id = CLR_RULE_NO_KICK;
    packing.rule.kick_profile_id = CLR_KICK_NO_KICK;
    packing.rule.spawn_profile_id = CLR_SPAWN_STANDARD_10;
    packing.goal = CLR_GOAL_CLEAR_TO_EMPTY;
    packing.count_policy = CLR_COUNT_ALL;
    packing.objective = CLR_OBJECTIVE_ALL;

    uint8_t columns[1] = {0};
    clr_buildup_problem problem =
        buildup_test_build_problem_from_candidate(packing, buildup_test_o_candidate_for_columns(search_layout, columns, 1));
    clr_buildup_verification verification;

    EXPECT_BUILDUP_STATUS(clr_buildup_worker_verify(&problem, &verification),
                          CLR_BUILDUP_OK);
    EXPECT_U64(verification.accepted, 1);
    EXPECT_U64(verification.variant.final_board, 0);
    EXPECT_U64(verification.variant.cleared_lines, 2);
}void failed_memo_hash_collision_does_not_merge_distinct_hold_states(void) {
    clr_buildup_bfs_state left_state = buildup_test_bfs_state();
    clr_buildup_bfs_state right_state = left_state;
    right_state.hold_automaton_state.bag_epoch++;
    clr_buildup_memo_key left = clearra_buildup_memo_key_from_bfs_state(
        buildup_test_full_cache_identity(), &left_state);
    clr_buildup_memo_key right = clearra_buildup_memo_key_from_bfs_state(
        buildup_test_full_cache_identity(), &right_state);

    EXPECT_TRUE(!clearra_buildup_memo_key_matches_bucket(
        &left, UINT64_C(0x55), &right, UINT64_C(0x55)));
}
void failed_memo_compares_piece_source_id_exactly(void) {
    clr_buildup_bfs_state left_state = buildup_test_bfs_state();
    clr_buildup_bfs_state right_state = left_state;
    right_state.hold_automaton_state.piece_source_id++;
    clr_buildup_memo_key left = clearra_buildup_memo_key_from_bfs_state(
        buildup_test_full_cache_identity(), &left_state);
    clr_buildup_memo_key right = clearra_buildup_memo_key_from_bfs_state(
        buildup_test_full_cache_identity(), &right_state);
    EXPECT_TRUE(!clearra_buildup_memo_key_equals_exact(&left, &right));
}
void failed_memo_compares_bag_epoch_exactly(void) {
    clr_buildup_bfs_state left_state = buildup_test_bfs_state();
    clr_buildup_bfs_state right_state = left_state;
    right_state.hold_automaton_state.bag_epoch++;
    clr_buildup_memo_key left = clearra_buildup_memo_key_from_bfs_state(
        buildup_test_full_cache_identity(), &left_state);
    clr_buildup_memo_key right = clearra_buildup_memo_key_from_bfs_state(
        buildup_test_full_cache_identity(), &right_state);
    EXPECT_TRUE(!clearra_buildup_memo_key_equals_exact(&left, &right));
}
void failed_memo_compares_bag_remainder_key_exactly(void) {
    clr_buildup_bfs_state left_state = buildup_test_bfs_state();
    clr_buildup_bfs_state right_state = left_state;
    right_state.hold_automaton_state.bag_remainder_key++;
    clr_buildup_memo_key left = clearra_buildup_memo_key_from_bfs_state(
        buildup_test_full_cache_identity(), &left_state);
    clr_buildup_memo_key right = clearra_buildup_memo_key_from_bfs_state(
        buildup_test_full_cache_identity(), &right_state);
    EXPECT_TRUE(!clearra_buildup_memo_key_equals_exact(&left, &right));
}
void failed_memo_compares_provenance_exactly(void) {
    clr_buildup_bfs_state left_state = buildup_test_bfs_state();
    clr_buildup_bfs_state right_state = left_state;
    right_state.hold_automaton_state.provenance_id++;
    clr_buildup_memo_key left = clearra_buildup_memo_key_from_bfs_state(
        buildup_test_full_cache_identity(), &left_state);
    clr_buildup_memo_key right = clearra_buildup_memo_key_from_bfs_state(
        buildup_test_full_cache_identity(), &right_state);
    EXPECT_TRUE(!clearra_buildup_memo_key_equals_exact(&left, &right));
}
void failed_memo_compares_hold_piece_and_empty_exactly(void) {
    clr_buildup_bfs_state base_state = buildup_test_bfs_state();
    clr_buildup_bfs_state piece_state = base_state;
    clr_buildup_bfs_state empty_state = base_state;
    piece_state.hold_automaton_state.hold_piece = CLR_PIECE_I;
    empty_state.hold_automaton_state.hold_empty =
        (uint8_t)!base_state.hold_automaton_state.hold_empty;
    clr_buildup_memo_key base = clearra_buildup_memo_key_from_bfs_state(
        buildup_test_full_cache_identity(), &base_state);
    clr_buildup_memo_key piece = clearra_buildup_memo_key_from_bfs_state(
        buildup_test_full_cache_identity(), &piece_state);
    clr_buildup_memo_key empty = clearra_buildup_memo_key_from_bfs_state(
        buildup_test_full_cache_identity(), &empty_state);
    EXPECT_TRUE(!clearra_buildup_memo_key_equals_exact(&base, &piece));
    EXPECT_TRUE(!clearra_buildup_memo_key_equals_exact(&base, &empty));
}
void failed_memo_compares_cursor_exactly(void) {
    clr_buildup_bfs_state left_state = buildup_test_bfs_state();
    clr_buildup_bfs_state right_state = left_state;
    right_state.hold_automaton_state.cursor++;
    right_state.piece_source_cursor++;
    clr_buildup_memo_key left = clearra_buildup_memo_key_from_bfs_state(
        buildup_test_full_cache_identity(), &left_state);
    clr_buildup_memo_key right = clearra_buildup_memo_key_from_bfs_state(
        buildup_test_full_cache_identity(), &right_state);
    EXPECT_TRUE(!clearra_buildup_memo_key_equals_exact(&left, &right));
}
void reachability_capacity_exceeded_is_incomplete_not_impossible(void) {
    clr_buildup_status status = clearra_buildup_status_from_reachability_status(
        CLEARRA_REACHABILITY_CAPACITY_EXCEEDED);
    EXPECT_BUILDUP_STATUS(status, CLR_BUILDUP_CAPACITY_EXCEEDED);
    EXPECT_U64(
        clearra_buildup_branch_outcome_for_status(status),
        CLEARRA_BUILDUP_BRANCH_INCOMPLETE);
}
void reachability_invalid_kick_table_is_fatal_not_unreachable(void) {
    ClearraBoard64Layout layout = buildup_test_standard_10x4_layout();
    clr_buildup_problem problem = {0};
    problem.rule = buildup_test_rule_descriptor(CLR_RULE_SRS, UINT32_C(0xffff));
    clr_buildup_operation operation = {0};
    operation.piece = CLR_PIECE_T;
    operation.rotation = CLEARRA_ROTATION_RIGHT;
    operation.x = 0;
    operation.y = 0;
    clr_buildup_status status = clearra_buildup_reachability_bridge_accepts(
        &problem, layout, clearra_board64_empty(), &operation, 0, 0);
    EXPECT_BUILDUP_STATUS(status, CLR_BUILDUP_INVALID_PROBLEM);
    EXPECT_U64(
        clearra_buildup_branch_outcome_for_status(status),
        CLEARRA_BUILDUP_BRANCH_FATAL);
}
void incomplete_branch_prevents_failed_memo_insert(void) {
    ClearraBuildUpSearchContext context = {0};
    ClearraBuildUpState state = {0};
    clearra_buildup_search_record_failure(
        &context, CLR_BUILDUP_CAPACITY_EXCEEDED, 0u);
    clearra_buildup_search_failed_memo_insert(&context, &state, 1u);
    EXPECT_U64(context.completion_memo.count, 0u);
}
void fatal_branch_prevents_failed_memo_insert(void) {
    ClearraBuildUpSearchContext context = {0};
    ClearraBuildUpState state = {0};
    clearra_buildup_search_record_failure(
        &context, CLR_BUILDUP_INVALID_PROBLEM, 0u);
    clearra_buildup_search_failed_memo_insert(&context, &state, 1u);
    EXPECT_U64(context.completion_memo.count, 0u);
}
void failed_memo_requires_all_branches_exhaustively_rejected(void) {
    clr_buildup_problem problem = clr_buildup_problem_from_packing(
        buildup_test_buildup_packing_problem(2u, 1u, 0u));
    ClearraBuildUpSearchContext context = {0};
    context.problem = &problem;
    clearra_buildup_completion_memo_init(
        &context.completion_memo, &problem);
    ClearraBuildUpState state = clearra_buildup_state_initial(&problem);
    clearra_buildup_search_record_failure(
        &context, CLR_BUILDUP_COLLISION, 0u);
    clearra_buildup_search_record_failure(
        &context, CLR_BUILDUP_REACHABILITY_IMPOSSIBLE, 0u);
    clearra_buildup_search_failed_memo_insert(&context, &state, 1u);
    EXPECT_U64(context.completion_memo.count, 1u);
    clearra_buildup_completion_memo_release(
        &context.completion_memo);
}
void build_variant_preserves_packing_candidate_id(void) {
    clr_buildup_problem problem = clr_buildup_problem_from_packing(
        buildup_test_buildup_packing_problem(2u, 1u, 0u));
    problem.candidate_id = UINT64_C(0x1234);
    problem.canonical_operation_set_id = UINT64_C(0x5678);
    problem.operation_set.operation_count = 1u;
    problem.operation_set.operations[0].piece = CLR_PIECE_O;
    problem.operation_set.operations[0].operation_id = 3u;
    problem.operation_set.operations[0].mask = UINT64_C(0xf);
    ClearraBuildUpState state = clearra_buildup_state_initial(&problem);
    clr_build_variant_view variant = {0};

    clearra_build_variant_from_state(&problem, &state, &variant);

    EXPECT_U64(variant.candidate_id, UINT64_C(0x1234));
    EXPECT_U64(variant.canonical_operation_set_id, UINT64_C(0x5678));
    EXPECT_TRUE(variant.operation_set_hash != 0u);
}
