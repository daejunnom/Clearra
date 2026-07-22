#include "packing_tests_support.h"

static clr_packing_problem packing_problem(
    ClearraBoard64Layout layout,
    uint64_t initial_mask,
    uint64_t target_mask,
    const uint8_t *pieces,
    uint8_t piece_count) {
    clr_packing_problem problem = clr_packing_problem_zero();
    problem.problem_kind = CLR_PROBLEM_OPENING_PC;
    problem.max_pieces = piece_count;
    problem.board.width = layout.width;
    problem.board.visible_height = layout.height;
    problem.board.search_height = layout.height;
    problem.board.initial_mask = initial_mask;
    problem.board.backend_kind = CLR_BOARD_BACKEND_BOARD64;
    problem.board.cell_count = layout.cell_count;
    problem.goal_region_mask = target_mask;
    problem.required_fill_mask = target_mask & ~initial_mask;
    problem.exact_pieces = piece_count;
    problem.piece_window =
        clearra_piece_window_descriptor(piece_count, piece_count, true);
    problem.piece_multiset_window =
        clearra_piece_multiset_window_from_pieces(pieces, piece_count);
    problem.piece_source = clearra_piece_source_descriptor_fixed_queue(
        UINT64_C(1),
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE,
        piece_count,
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

void packing_candidate_is_not_solution_before_buildup(void) {
    ClearraPackingFixtureState candidate = {0};
    candidate.placed_pieces = 5;
    candidate.cleared_lines = 2;
    EXPECT_U64(candidate.placed_pieces, 5u);
    EXPECT_U64(candidate.cleared_lines, 2u);
}

void two_line_empty_board_packing_candidates_generated(void) {
    static ClearraPackingCandidateBuffer buffer;
    const ClearraBoard64Layout layout = packing_test_two_by_two_layout();
    const uint8_t pieces[1] = {CLR_PIECE_O};
    uint64_t target_mask = 0;

    EXPECT_STATUS(
        clearra_packing_target_mask_for_lines(layout, 2, &target_mask),
        CLEARRA_PACKING_OK);
    EXPECT_STATUS(
        clearra_packing_enumerator_cpu_generate(
            layout, clearra_board64_empty(), 2, pieces, 1, &buffer),
        CLEARRA_PACKING_OK);
    EXPECT_U64(buffer.count, 1u);
    EXPECT_U64(buffer.final_boards[0], 0u);
    EXPECT_U64(buffer.shape_masks[0], target_mask);
    EXPECT_U64(buffer.placed_counts[0], 1u);
}

void problem_descriptor_packing_candidates_generated(void) {
    static ClearraPackingCandidateBuffer buffer;
    const ClearraBoard64Layout layout = packing_test_two_by_two_layout();
    const uint8_t pieces[1] = {CLR_PIECE_O};
    clr_packing_problem problem =
        packing_problem(layout, 0u, layout.all_cells_mask, pieces, 1u);

    EXPECT_STATUS(
        clearra_packing_enumerator_cpu_generate_problem(&problem, &buffer),
        CLEARRA_PACKING_OK);
    EXPECT_U64(buffer.count, 1u);
    EXPECT_U64(buffer.geometry_variant_domains[0], 1u);
}

void problem_descriptor_capacity_preserves_candidates_and_resource_report(void) {
    static ClearraPackingCandidateBuffer buffer;
    ClearraBoard64Layout layout;
    clr_resource_report report;
    const uint8_t pieces[5] = {
        CLR_PIECE_I, CLR_PIECE_I, CLR_PIECE_O, CLR_PIECE_O, CLR_PIECE_O};

    EXPECT_U64(clearra_board64_make_layout(10u, 2u, &layout), CLEARRA_BOARD64_OK);
    clr_packing_problem problem =
        packing_problem(layout, 0u, layout.all_cells_mask, pieces, 5u);
    problem.budget.max_results = 1u;

    EXPECT_STATUS(
        clearra_packing_enumerator_cpu_generate_problem_with_resource_report(
            &problem, &buffer, &report),
        CLEARRA_PACKING_CAPACITY_EXCEEDED);
    EXPECT_U64(buffer.count, 1u);
    EXPECT_TRUE(report.truncated != 0u);
    EXPECT_TRUE(report.probability_complete == 0u);
    EXPECT_U64(
        report.truncation_reason,
        CLR_RESOURCE_TRUNCATION_CANDIDATE_BUDGET_EXCEEDED);
}

void packing_problem_allows_piece_multiset_supply_superset_of_search_window(void) {
    const ClearraBoard64Layout layout = packing_test_standard_two_line_layout();
    const uint8_t pieces[7] = {
        CLR_PIECE_I,
        CLR_PIECE_O,
        CLR_PIECE_T,
        CLR_PIECE_S,
        CLR_PIECE_Z,
        CLR_PIECE_J,
        CLR_PIECE_L,
    };
    clr_packing_problem problem =
        packing_problem(layout, 0u, layout.all_cells_mask, pieces, 7u);
    problem.max_pieces = 5u;
    problem.exact_pieces = 5u;
    problem.piece_window = clearra_piece_window_descriptor(5u, 5u, true);
    problem.piece_multiset_window.exact_count = 5u;

    EXPECT_TRUE(clr_packing_problem_is_valid(&problem));
}

void problem_descriptor_uses_search_height_for_layout_when_visible_height_differs(void) {
    static ClearraPackingCandidateBuffer buffer;
    const ClearraBoard64Layout layout = packing_test_two_by_two_layout();
    const uint8_t pieces[1] = {CLR_PIECE_O};
    clr_packing_problem problem =
        packing_problem(layout, 0u, layout.all_cells_mask, pieces, 1u);
    problem.board.visible_height = 1u;
    problem.board.search_height = 2u;

    EXPECT_STATUS(
        clearra_packing_enumerator_cpu_generate_problem(&problem, &buffer),
        CLEARRA_PACKING_OK);
    EXPECT_U64(buffer.count, 1u);
    EXPECT_U64(buffer.shape_masks[0], layout.all_cells_mask);
}
