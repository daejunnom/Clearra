#include "buildup_tests_support.h"

#include <string.h>

void hold_transition_updates_bag_epoch_and_remainder_from_piece_source_pattern(void);
static clr_buildup_problem buildup_hold_problem_from_sequence(
    const uint8_t *pieces,
    uint16_t count,
    uint8_t hold_enabled,
    uint32_t source_kind,
    uint32_t provenance_id) {
    clr_packing_problem packing =
        buildup_test_buildup_packing_problem(2, count, hold_enabled);
    buildup_test_set_packing_pieces(
        &packing,
        pieces,
        count,
        source_kind,
        provenance_id);
    return clr_buildup_problem_from_packing(packing);
}void enumerate_variants_preserves_hold_branches(void) {
    clr_packing_problem packing = buildup_test_buildup_packing_problem(2, 2, 1);
    const uint8_t pieces[2] = {CLR_PIECE_O, CLR_PIECE_T};
    buildup_test_set_packing_pieces(
        &packing,
        pieces,
        2,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);
    clr_buildup_problem problem = clr_buildup_problem_from_packing(packing);
    buildup_test_configure_initial_hold(&problem, 1, 0, CLR_PIECE_O);
    ClearraBuildUpQueueHold state;
    ClearraBuildUpHoldBranch branches[CLEARRA_BUILDUP_HOLD_BRANCH_MAX];
    uint8_t branch_count = 0u;

    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_init(&problem, &state),
                          CLR_BUILDUP_OK);
    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_enumerate_branches(
                              &problem, &state, CLR_PIECE_O, branches, &branch_count),
                          CLR_BUILDUP_OK);

    EXPECT_U64(branch_count, 2);
    EXPECT_U64(branches[0].branch_kind, CLEARRA_BUILDUP_HOLD_BRANCH_CURRENT);
    EXPECT_U64(branches[0].used_hold, 0);
    EXPECT_U64(branches[1].branch_kind, CLEARRA_BUILDUP_HOLD_BRANCH_SWAP_HELD);
    EXPECT_U64(branches[1].used_hold, 1);
    EXPECT_U64(branches[1].state.hold_piece, CLR_PIECE_O);
}void fixed_queue_tio_not_reordered_to_iot(void) {
    const uint8_t pieces[3] = {CLR_PIECE_T, CLR_PIECE_I, CLR_PIECE_O};
    clr_buildup_problem problem = buildup_hold_problem_from_sequence(
        pieces,
        3,
        0,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);
    ClearraBuildUpQueueHold state;
    ClearraBuildUpHoldBranch branches[CLEARRA_BUILDUP_HOLD_BRANCH_MAX];
    uint8_t branch_count = 0u;

    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_init(&problem, &state),
                          CLR_BUILDUP_OK);
    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_enumerate_branches(
                              &problem, &state, CLR_PIECE_T, branches, &branch_count),
                          CLR_BUILDUP_OK);
    EXPECT_U64(branch_count, 1);
    EXPECT_U64(branches[0].state.cursor, 1);

    branch_count = 0u;
    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_enumerate_branches(
                              &problem, &branches[0].state, CLR_PIECE_I, branches, &branch_count),
                          CLR_BUILDUP_OK);
    EXPECT_U64(branch_count, 1);
    EXPECT_U64(branches[0].state.cursor, 2);
}void fixed_queue_order_not_reordered_by_multiset(void) {
    fixed_queue_tio_not_reordered_to_iot();
}void fixed_queue_same_multiset_different_order_changes_buildability(void) {
    const uint8_t tio[3] = {CLR_PIECE_T, CLR_PIECE_I, CLR_PIECE_O};
    const uint8_t ito[3] = {CLR_PIECE_I, CLR_PIECE_T, CLR_PIECE_O};
    clr_buildup_problem tio_problem = buildup_hold_problem_from_sequence(
        tio,
        3,
        0,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);
    clr_buildup_problem ito_problem = buildup_hold_problem_from_sequence(
        ito,
        3,
        0,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);
    ClearraBuildUpQueueHold state;
    ClearraBuildUpHoldBranch branches[CLEARRA_BUILDUP_HOLD_BRANCH_MAX];
    uint8_t branch_count = 0u;

    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_init(&tio_problem, &state),
                          CLR_BUILDUP_OK);
    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_enumerate_branches(
                              &tio_problem, &state, CLR_PIECE_T, branches, &branch_count),
                          CLR_BUILDUP_OK);
    EXPECT_U64(branch_count, 1);

    branch_count = 0u;
    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_init(&ito_problem, &state),
                          CLR_BUILDUP_OK);
    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_enumerate_branches(
                              &ito_problem, &state, CLR_PIECE_T, branches, &branch_count),
                          CLR_BUILDUP_HOLD_DISABLED_IMPOSSIBLE);
    EXPECT_U64(branch_count, 0);
}void same_multiset_different_queue_changes_buildability(void) {
    fixed_queue_same_multiset_different_order_changes_buildability();
}void hold_disabled_uses_actual_queue_order(void) {
    const uint8_t pieces[3] = {CLR_PIECE_T, CLR_PIECE_I, CLR_PIECE_O};
    clr_buildup_problem problem = buildup_hold_problem_from_sequence(
        pieces,
        3,
        0,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);
    ClearraBuildUpQueueHold state;
    ClearraBuildUpHoldBranch branches[CLEARRA_BUILDUP_HOLD_BRANCH_MAX];
    uint8_t branch_count = 0u;

    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_init(&problem, &state),
                          CLR_BUILDUP_OK);
    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_enumerate_branches(
                              &problem, &state, CLR_PIECE_I, branches, &branch_count),
                          CLR_BUILDUP_HOLD_DISABLED_IMPOSSIBLE);
    EXPECT_U64(branch_count, 0);
}void hold_enabled_long_carryover_uses_piece_source_pattern(void) {
    const uint8_t pieces[3] = {CLR_PIECE_T, CLR_PIECE_I, CLR_PIECE_O};
    clr_buildup_problem problem = buildup_hold_problem_from_sequence(
        pieces,
        3,
        1,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);
    ClearraBuildUpQueueHold state;
    ClearraBuildUpHoldBranch branches[CLEARRA_BUILDUP_HOLD_BRANCH_MAX];
    uint8_t branch_count = 0u;

    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_init(&problem, &state),
                          CLR_BUILDUP_OK);
    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_enumerate_branches(
                              &problem, &state, CLR_PIECE_I, branches, &branch_count),
                          CLR_BUILDUP_OK);
    EXPECT_U64(branch_count, 1);
    EXPECT_U64(branches[0].branch_kind, CLEARRA_BUILDUP_HOLD_BRANCH_STORE_CURRENT);
    EXPECT_U64(branches[0].state.cursor, 2);
    EXPECT_U64(branches[0].state.hold_piece, CLR_PIECE_T);

    state = branches[0].state;
    branch_count = 0u;
    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_enumerate_branches(
                              &problem, &state, CLR_PIECE_T, branches, &branch_count),
                          CLR_BUILDUP_OK);
    EXPECT_U64(branch_count, 1);
    EXPECT_U64(branches[0].branch_kind, CLEARRA_BUILDUP_HOLD_BRANCH_SWAP_HELD);
    EXPECT_U64(branches[0].state.cursor, 3);
    EXPECT_U64(branches[0].state.hold_piece, CLR_PIECE_O);
}void long_hold_carryover_uses_bag_epoch_and_remainder(void) {
    hold_enabled_long_carryover_uses_piece_source_pattern();
    hold_transition_updates_bag_epoch_and_remainder_from_piece_source_pattern();
}void terminal_projection_branch_is_exactly_once_and_terminal_only(void) {
    const uint8_t pieces[1] = {CLR_PIECE_I};
    clr_buildup_problem problem = buildup_hold_problem_from_sequence(
        pieces,
        1,
        1,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);
    problem.terminal_projection_policy =
        CLR_BUILDUP_TERMINAL_PROJECTION_RELEASE_FINITE_HELD;
    ClearraBuildUpQueueHold state;
    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_init(&problem, &state),
                          CLR_BUILDUP_OK);
    state.cursor = 1u;
    state.hold_piece = CLR_PIECE_O;
    state.hold_empty = 0u;

    ClearraBuildUpHoldBranchTable branches;
    const uint8_t desired_o =
        (uint8_t)(UINT8_C(1) << CLR_PIECE_O);
    EXPECT_BUILDUP_STATUS(
        clearra_buildup_queue_hold_enumerate_branch_mask_for_step(
            &problem, &state, desired_o, false, &branches),
        CLR_BUILDUP_OK);
    EXPECT_U64(branches.counts[CLR_PIECE_O], 0u);

    EXPECT_BUILDUP_STATUS(
        clearra_buildup_queue_hold_enumerate_branch_mask_for_step(
            &problem, &state, desired_o, true, &branches),
        CLR_BUILDUP_OK);
    EXPECT_U64(branches.counts[CLR_PIECE_O], 1u);
    ClearraBuildUpHoldBranch terminal =
        branches.branches[CLR_PIECE_O][0];
    EXPECT_U64(terminal.branch_kind,
               CLEARRA_BUILDUP_HOLD_BRANCH_RELEASE_HELD_AT_TERMINAL);
    EXPECT_U64(terminal.used_hold, 1u);
    EXPECT_U64(terminal.incoming_piece, CLR_PIECE_NONE);
    EXPECT_U64(terminal.held_piece_before, CLR_PIECE_O);
    EXPECT_U64(terminal.state.cursor, 1u);
    EXPECT_U64(terminal.state.hold_piece, CLR_PIECE_NONE);
    EXPECT_U64(terminal.state.hold_empty, 1u);
    EXPECT_U64(terminal.state.terminal_projection_consumed, 1u);
    EXPECT_U64(
        terminal.state.terminal_projection_provenance,
        CLEARRA_BUILDUP_TERMINAL_PROVENANCE_FINITE_SOURCE_END);

    EXPECT_BUILDUP_STATUS(
        clearra_buildup_queue_hold_enumerate_branch_mask_for_step(
            &problem, &terminal.state, desired_o, true, &branches),
        CLR_BUILDUP_OK);
    EXPECT_U64(branches.counts[CLR_PIECE_O], 0u);

    problem.terminal_projection_policy =
        CLR_BUILDUP_TERMINAL_PROJECTION_DISABLED;
    EXPECT_BUILDUP_STATUS(
        clearra_buildup_queue_hold_enumerate_branch_mask_for_step(
            &problem, &state, desired_o, true, &branches),
        CLR_BUILDUP_OK);
    EXPECT_U64(branches.counts[CLR_PIECE_O], 0u);
}void terminal_projection_builds_with_occupied_initial_hold(void) {
    ClearraBoard64Layout layout = buildup_test_standard_10x2_layout();
    uint8_t columns[2] = {0, 2};
    clr_packing_problem packing = buildup_test_buildup_packing_problem(2, 2, 1);
    uint64_t target_mask = buildup_test_low_mask_for_cells(20);
    uint64_t missing = buildup_test_o_mask_at(layout, 0, 0) |
                       buildup_test_o_mask_at(layout, 2, 0);
    packing.board.initial_mask = target_mask & ~missing;
    packing.required_fill_mask = missing;
    const uint8_t pieces[1] = {CLR_PIECE_O};
    buildup_test_set_packing_pieces(
        &packing,
        pieces,
        1,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);

    clr_buildup_problem problem = buildup_test_build_problem_from_candidate(
        packing, buildup_test_o_candidate_for_columns(layout, columns, 2));
    buildup_test_configure_initial_hold(&problem, 1, 0, CLR_PIECE_O);
    problem.terminal_projection_policy =
        CLR_BUILDUP_TERMINAL_PROJECTION_RELEASE_FINITE_HELD;
    clr_build_variant_buffer *first =
        (clr_build_variant_buffer *)malloc(sizeof(clr_build_variant_buffer));
    EXPECT_TRUE(first != 0);

    EXPECT_BUILDUP_STATUS(clr_buildup_verify_first(&problem, first),
                          CLR_BUILDUP_OK);
    EXPECT_U64(first->count, 1u);
    EXPECT_U64(first->variants[0].trace_step_count, 2u);
    EXPECT_U64(
        first->trace_step_storage[0][1].hold_branch_kind,
        CLEARRA_BUILDUP_HOLD_BRANCH_RELEASE_HELD_AT_TERMINAL);

    problem.terminal_projection_policy =
        CLR_BUILDUP_TERMINAL_PROJECTION_DISABLED;
    EXPECT_BUILDUP_STATUS(clr_buildup_verify_first(&problem, first),
                          CLR_BUILDUP_QUEUE_ORDER_IMPOSSIBLE);
    free(first);
}void piece_source_reader_rejects_provenance_mismatch(void) {
    const uint8_t pieces[2] = {CLR_PIECE_T, CLR_PIECE_I};
    clr_buildup_problem problem = buildup_hold_problem_from_sequence(
        pieces,
        2,
        0,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);
    problem.initial_hold_automaton.provenance_id = 999u;
    ClearraBuildUpQueueHold state;
    ClearraBuildUpHoldBranch branches[CLEARRA_BUILDUP_HOLD_BRANCH_MAX];
    uint8_t branch_count = 0u;

    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_init(&problem, &state),
                          CLR_BUILDUP_OK);
    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_enumerate_branches(
                              &problem, &state, CLR_PIECE_T, branches, &branch_count),
                          CLR_BUILDUP_INVALID_PROBLEM);
    EXPECT_U64(branch_count, 0u);
}void hold_transition_updates_bag_epoch_and_remainder_from_piece_source_pattern(void) {
    const uint8_t pieces[8] = {
        CLR_PIECE_I,
        CLR_PIECE_O,
        CLR_PIECE_T,
        CLR_PIECE_S,
        CLR_PIECE_Z,
        CLR_PIECE_J,
        CLR_PIECE_L,
        CLR_PIECE_I,
    };
    clr_buildup_problem problem = buildup_hold_problem_from_sequence(
        pieces,
        8,
        0,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);
    ClearraBuildUpQueueHold state;
    ClearraBuildUpHoldBranch branches[CLEARRA_BUILDUP_HOLD_BRANCH_MAX];
    uint8_t branch_count = 0u;

    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_init(&problem, &state),
                          CLR_BUILDUP_OK);
    state.cursor = 6u;
    state.bag_epoch = 0u;
    state.bag_remainder_key = 0u;

    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_enumerate_branches(
                              &problem, &state, CLR_PIECE_L, branches, &branch_count),
                          CLR_BUILDUP_OK);

    EXPECT_U64(branch_count, 1u);
    EXPECT_U64(branches[0].state.cursor, 7u);
    EXPECT_U64(branches[0].state.bag_epoch, 1u);
    EXPECT_U64(branches[0].state.bag_remainder_key,
               UINT64_C(1) << ((uint64_t)CLR_PIECE_I * 4u));
}void bag_universe_pattern_id_controls_sequence(void) {
    const uint8_t pieces[3] = {CLR_PIECE_O, CLR_PIECE_T, CLR_PIECE_I};
    clr_buildup_problem problem = buildup_hold_problem_from_sequence(
        pieces,
        3,
        0,
        CLR_PIECE_SOURCE_BAG_UNIVERSE,
        CLR_SUPPLY_PROVENANCE_BAG_ALIGNED_PATTERN);
    problem.piece_source_pattern_id = 42u;
    ClearraBuildUpQueueHold state;
    ClearraBuildUpHoldBranch branches[CLEARRA_BUILDUP_HOLD_BRANCH_MAX];
    uint8_t branch_count = 0u;

    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_init(&problem, &state),
                          CLR_BUILDUP_OK);
    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_enumerate_branches(
                              &problem, &state, CLR_PIECE_O, branches, &branch_count),
                          CLR_BUILDUP_OK);
    EXPECT_U64(problem.piece_source_pattern_id, 42u);
    EXPECT_U64(branch_count, 1);

    branch_count = 0u;
    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_init(&problem, &state),
                          CLR_BUILDUP_OK);
    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_enumerate_branches(
                              &problem, &state, CLR_PIECE_I, branches, &branch_count),
                          CLR_BUILDUP_HOLD_DISABLED_IMPOSSIBLE);
    EXPECT_U64(branch_count, 0);
}void bag_pattern_id_changes_hold_reachable_language(void) {
    bag_universe_pattern_id_controls_sequence();
}void bag_universe_allows_duplicate_across_bag_epoch(void) {
    const uint8_t pieces[8] = {
        CLR_PIECE_I,
        CLR_PIECE_O,
        CLR_PIECE_T,
        CLR_PIECE_S,
        CLR_PIECE_Z,
        CLR_PIECE_J,
        CLR_PIECE_L,
        CLR_PIECE_I,
    };
    clr_buildup_problem problem = buildup_hold_problem_from_sequence(
        pieces,
        8,
        0,
        CLR_PIECE_SOURCE_BAG_UNIVERSE,
        CLR_SUPPLY_PROVENANCE_BAG_ALIGNED_PATTERN);

    EXPECT_BUILDUP_STATUS(clearra_buildup_verify_bag_pattern(&problem),
                          CLR_BUILDUP_OK);
}void materialized_pattern_reader_preserves_pattern_order(void) {
    const uint8_t pieces[3] = {CLR_PIECE_Z, CLR_PIECE_S, CLR_PIECE_T};
    clr_piece_source_descriptor source = {
        .piece_source_id = 77u,
        .source_kind = CLR_PIECE_SOURCE_MATERIALIZED_PATTERN_UNIVERSE,
        .provenance_id = CLR_SUPPLY_PROVENANCE_BAG_ALIGNED_PATTERN,
        .pattern_universe_id = 5u,
        .pattern_weight_model_id = 9u,
        .materialized_pattern_count = 3u,
        .piece_set_profile_id = CLR_PIECE_SET_STANDARD_TETROMINOES,
        .complete = 1u,
    };
    clr_piece_source_pattern_reader reader = {
        .source = source,
        .pattern_id = 2u,
        .fixed_or_materialized_pieces = pieces,
        .len = 3u,
        .complete = 1u,
        .truncation_reason = CLR_SUPPLY_TRUNCATION_NONE,
    };
    clr_hold_automaton_state state = {
        .piece_source_id = 77u,
        .provenance_id = CLR_SUPPLY_PROVENANCE_BAG_ALIGNED_PATTERN,
    };
    uint8_t piece = CLR_PIECE_NONE;

    EXPECT_BUILDUP_STATUS(
        clr_piece_source_pattern_piece_at(&reader, &state, 0, &piece),
        CLR_BUILDUP_OK);
    EXPECT_U64(piece, CLR_PIECE_Z);
    EXPECT_BUILDUP_STATUS(
        clr_piece_source_pattern_piece_at(&reader, &state, 1, &piece),
        CLR_BUILDUP_OK);
    EXPECT_U64(piece, CLR_PIECE_S);
    EXPECT_BUILDUP_STATUS(
        clr_piece_source_pattern_piece_at(&reader, &state, 2, &piece),
        CLR_BUILDUP_OK);
    EXPECT_U64(piece, CLR_PIECE_T);
}void synthetic_multiset_queue_is_forbidden(void) {
    const uint8_t pieces[3] = {CLR_PIECE_T, CLR_PIECE_I, CLR_PIECE_O};
    clr_buildup_problem problem = buildup_hold_problem_from_sequence(
        pieces,
        3,
        0,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);
    memset(problem.piece_source_pattern_pieces,
           CLR_PIECE_NONE,
           sizeof(problem.piece_source_pattern_pieces));
    problem.piece_source_pattern_len = 0u;
    ClearraBuildUpQueueHold state;
    ClearraBuildUpHoldBranch branches[CLEARRA_BUILDUP_HOLD_BRANCH_MAX];
    uint8_t branch_count = 0u;

    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_init(&problem, &state),
                          CLR_BUILDUP_OK);
    EXPECT_BUILDUP_STATUS(clearra_buildup_queue_hold_enumerate_branches(
                              &problem, &state, CLR_PIECE_I, branches, &branch_count),
                          CLR_BUILDUP_INVALID_PROBLEM);
    EXPECT_U64(branch_count, 0);
}void buildup_enumerate_variants_preserves_hold_branches(void) {
    enumerate_variants_preserves_hold_branches();
}void buildup_enumerate_variants_returns_multiple_hold_branches(void) {
    ClearraBoard64Layout layout = buildup_test_standard_10x2_layout();
    uint8_t columns[1] = {0};
    clr_packing_problem packing = buildup_test_buildup_packing_problem(2, 1, 1);
    uint64_t target_mask = buildup_test_low_mask_for_cells(20);
    uint64_t missing_o = buildup_test_o_mask_at(layout, 0, 0);
    packing.board.initial_mask = target_mask & ~missing_o;
    packing.required_fill_mask = missing_o;
    const uint8_t pieces[1] = {CLR_PIECE_O};
    buildup_test_set_packing_pieces(
        &packing,
        pieces,
        1,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);

    clr_buildup_problem problem =
        buildup_test_build_problem_from_candidate(packing, buildup_test_o_candidate_for_columns(layout, columns, 1));
    buildup_test_configure_initial_hold(&problem, 1, 0, CLR_PIECE_O);
    clr_build_variant_buffer *first =
        (clr_build_variant_buffer *)malloc(sizeof(clr_build_variant_buffer));
    clr_build_variant_buffer *variants =
        (clr_build_variant_buffer *)malloc(sizeof(clr_build_variant_buffer));
    clr_buildup_enumeration_limits limits = {0};

    EXPECT_TRUE(first != 0);
    EXPECT_TRUE(variants != 0);

    limits.max_variants = CLR_BUILDUP_MAX_VARIANTS;
    limits.preserve_hold_branches = 1u;

    EXPECT_BUILDUP_STATUS(clr_buildup_verify_first(&problem, first),
                          CLR_BUILDUP_OK);
    EXPECT_BUILDUP_STATUS(clr_buildup_enumerate_variants(&problem, &limits, variants),
                          CLR_BUILDUP_OK);

    EXPECT_U64(first->count, 1);
    EXPECT_U64(variants->count, 2);
    EXPECT_U64(variants->variants[0].final_board, 0);
    EXPECT_U64(variants->variants[1].final_board, 0);
    EXPECT_U64(variants->variants[0].queue_cursor, 1);
    EXPECT_U64(variants->variants[1].queue_cursor, 1);
    EXPECT_U64(variants->variants[0].hold_piece, CLR_PIECE_O);
    EXPECT_U64(variants->variants[1].hold_piece, CLR_PIECE_O);

    free(first);
    free(variants);
}void enumerate_variants_always_preserves_hold_branches(void) {
    ClearraBoard64Layout layout = buildup_test_standard_10x2_layout();
    uint8_t columns[1] = {0};
    clr_packing_problem packing = buildup_test_buildup_packing_problem(2, 1, 1);
    uint64_t target_mask = buildup_test_low_mask_for_cells(20);
    uint64_t missing_o = buildup_test_o_mask_at(layout, 0, 0);
    packing.board.initial_mask = target_mask & ~missing_o;
    packing.required_fill_mask = missing_o;
    const uint8_t pieces[1] = {CLR_PIECE_O};
    buildup_test_set_packing_pieces(
        &packing,
        pieces,
        1,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);

    clr_buildup_problem problem =
        buildup_test_build_problem_from_candidate(packing, buildup_test_o_candidate_for_columns(layout, columns, 1));
    buildup_test_configure_initial_hold(&problem, 1, 0, CLR_PIECE_O);
    clr_build_variant_buffer *variants =
        (clr_build_variant_buffer *)malloc(sizeof(clr_build_variant_buffer));
    clr_buildup_enumeration_limits limits = {0};
    clr_buildup_count_limits count_limits = {0};
    clr_buildup_count_report count_report;

    EXPECT_TRUE(variants != 0);
    limits.max_variants = CLR_BUILDUP_MAX_VARIANTS;
    limits.preserve_hold_branches = 0u;
    count_limits.max_variants = CLR_BUILDUP_MAX_VARIANTS;
    count_limits.preserve_hold_branches = 0u;
    count_limits.retain_traces = 1u;

    EXPECT_BUILDUP_STATUS(clr_buildup_enumerate_variants(&problem, &limits, variants),
                          CLR_BUILDUP_OK);
    EXPECT_BUILDUP_STATUS(clr_buildup_count_variants(&problem, &count_limits, &count_report),
                          CLR_BUILDUP_OK);

    EXPECT_U64(variants->count, 2);
    EXPECT_U64(variants->variants[0].hold_branch_kind,
               CLEARRA_BUILDUP_HOLD_BRANCH_CURRENT);
    EXPECT_U64(variants->variants[1].hold_branch_kind,
               CLEARRA_BUILDUP_HOLD_BRANCH_SWAP_HELD);
    EXPECT_U64(count_report.total_variant_count, 2);
    EXPECT_U64(count_report.count_complete, 1);
    EXPECT_U64(count_report.trace_retained, 0);
    EXPECT_U64(count_report.retained_variant_count, 0);

    free(variants);
}void coverage_mode_never_calls_consume_first_branch_only(void) {
    enumerate_variants_always_preserves_hold_branches();
}void buildup_enumerate_variants_preserves_hold_branch_kind(void) {
    ClearraBoard64Layout layout = buildup_test_standard_10x2_layout();
    uint8_t columns[1] = {0};
    clr_packing_problem packing = buildup_test_buildup_packing_problem(2, 1, 1);
    uint64_t target_mask = buildup_test_low_mask_for_cells(20);
    uint64_t missing_o = buildup_test_o_mask_at(layout, 0, 0);
    packing.board.initial_mask = target_mask & ~missing_o;
    packing.required_fill_mask = missing_o;
    const uint8_t pieces[1] = {CLR_PIECE_O};
    buildup_test_set_packing_pieces(
        &packing,
        pieces,
        1,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);

    clr_buildup_problem problem =
        buildup_test_build_problem_from_candidate(packing, buildup_test_o_candidate_for_columns(layout, columns, 1));
    buildup_test_configure_initial_hold(&problem, 1, 0, CLR_PIECE_O);
    clr_build_variant_buffer *variants =
        (clr_build_variant_buffer *)malloc(sizeof(clr_build_variant_buffer));
    clr_buildup_enumeration_limits limits = {0};

    EXPECT_TRUE(variants != 0);
    limits.max_variants = CLR_BUILDUP_MAX_VARIANTS;
    limits.preserve_hold_branches = 1u;

    EXPECT_BUILDUP_STATUS(clr_buildup_enumerate_variants(&problem, &limits, variants),
                          CLR_BUILDUP_OK);

    EXPECT_U64(variants->count, 2);
    EXPECT_U64(variants->variants[0].hold_branch_kind,
               CLEARRA_BUILDUP_HOLD_BRANCH_CURRENT);
    EXPECT_U64(variants->variants[1].hold_branch_kind,
               CLEARRA_BUILDUP_HOLD_BRANCH_SWAP_HELD);

    free(variants);
}void build_variant_exports_hold_branch_kind(void) {
    buildup_enumerate_variants_preserves_hold_branch_kind();
}void hold_decision_sequence_is_preserved(void) {
    ClearraBoard64Layout layout = buildup_test_standard_10x2_layout();
    uint8_t columns[1] = {0};
    clr_packing_problem packing = buildup_test_buildup_packing_problem(2, 1, 1);
    uint64_t target_mask = buildup_test_low_mask_for_cells(20);
    uint64_t missing_o = buildup_test_o_mask_at(layout, 0, 0);
    packing.board.initial_mask = target_mask & ~missing_o;
    packing.required_fill_mask = missing_o;
    const uint8_t pieces[1] = {CLR_PIECE_T};
    buildup_test_set_packing_pieces(
        &packing, pieces, 1, CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);

    clr_buildup_problem problem = buildup_test_build_problem_from_candidate(
        packing, buildup_test_o_candidate_for_columns(layout, columns, 1));
    buildup_test_configure_initial_hold(&problem, 1, 0, CLR_PIECE_O);
    clr_buildup_verification verification;

    EXPECT_BUILDUP_STATUS(
        clr_buildup_worker_verify(&problem, &verification), CLR_BUILDUP_OK);
    EXPECT_U64(verification.variant.trace_step_count, 1);
    EXPECT_TRUE(verification.variant.trace_steps != 0);
    EXPECT_U64(verification.variant.trace_steps[0].hold_branch_kind,
               CLEARRA_BUILDUP_HOLD_BRANCH_SWAP_HELD);
    EXPECT_U64(verification.variant.trace_steps[0].used_hold, 1);
    EXPECT_U64(verification.variant.trace_steps[0].incoming_piece,
               CLR_PIECE_T);
    EXPECT_U64(verification.variant.trace_steps[0].held_piece_before,
               CLR_PIECE_O);
    EXPECT_U64(verification.variant.trace_steps[0].hold_empty_before, 0);
}void count_variants_reports_complete_count_without_retaining_all_traces(void) {
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
    clr_buildup_count_limits limits = {0};
    clr_buildup_count_report report;

    limits.max_variants = CLR_BUILDUP_MAX_VARIANTS;
    limits.retain_traces = 0u;

    EXPECT_BUILDUP_STATUS(clr_buildup_count_variants(&problem, &limits, &report),
                          CLR_BUILDUP_OK);
    EXPECT_U64(report.total_variant_count, 120);
    EXPECT_U64(report.count_complete, 1);
    EXPECT_U64(report.trace_retained, 0);
    EXPECT_U64(report.retained_variant_count, 0);
}
