#include "clr_problem.h"

#include <stdio.h>
#include <stdlib.h>

#define EXPECT_TRUE(EXPR)                                                   \
    do {                                                                    \
        if (!(EXPR)) {                                                      \
            fprintf(stderr, "%s:%d expected true\n", __FILE__, __LINE__);  \
            exit(1);                                                        \
        }                                                                   \
    } while (0)

#define EXPECT_FALSE(EXPR)                                                  \
    do {                                                                    \
        if ((EXPR)) {                                                       \
            fprintf(stderr, "%s:%d expected false\n", __FILE__, __LINE__); \
            exit(1);                                                        \
        }                                                                   \
    } while (0)

#define EXPECT_U64(EXPR, EXPECTED)                                          \
    do {                                                                    \
        uint64_t actual_value = (uint64_t)(EXPR);                           \
        uint64_t expected_value = (uint64_t)(EXPECTED);                     \
        if (actual_value != expected_value) {                               \
            fprintf(stderr, "%s:%d expected 0x%llx but got 0x%llx\n",      \
                    __FILE__, __LINE__, (unsigned long long)expected_value, \
                    (unsigned long long)actual_value);                      \
            exit(1);                                                        \
        }                                                                   \
    } while (0)
static void set_board_descriptor(
    clr_board_descriptor *descriptor,
    uint16_t width,
    uint16_t visible_height,
    uint16_t search_height,
    uint64_t initial_mask) {
    if (clr_board_descriptor_init(
            width, visible_height, search_height, initial_mask, 0, descriptor) !=
        CLR_BOARD_OK) {
        fprintf(stderr, "failed to initialize board descriptor\n");
        exit(1);
    }
}static clr_packing_problem valid_problem(void) {
    clr_packing_problem problem = clr_packing_problem_zero();
    problem.problem_kind = CLR_PROBLEM_SCENARIO_PC;
    problem.max_pieces = 5;
    set_board_descriptor(&problem.board, 10, 2, 4, UINT64_C(0x3f0));
    problem.goal_region_mask = (UINT64_C(1) << 20) - UINT64_C(1);
    problem.required_fill_mask = problem.goal_region_mask & ~problem.board.initial_mask;
    problem.piece_window.max_pieces = 5;
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
    problem.rule.rule_profile_id = CLR_RULE_SRS_PLUS;
    problem.rule.kick_profile_id = CLR_KICK_SRS_PLUS_180;
    problem.rule.spawn_profile_id = CLR_SPAWN_STANDARD_10;
    problem.budget.max_nodes = 100;
    problem.budget.max_results = 16;
    problem.backend.requested_backend = CLR_BACKEND_CPU;
    problem.backend.workers = 1;
    problem.backend.deterministic = 1;
    problem.backend.fallback_policy = CLR_BACKEND_FALLBACK_DENY;
    problem.goal = CLR_GOAL_CLEAR_TO_EMPTY;
    problem.count_policy = CLR_COUNT_ALL;
    problem.objective = CLR_OBJECTIVE_ALL;
    return problem;
}static void compact_problem_preserves_search_problem_fields(void) {
    clr_packing_problem problem = valid_problem();

    EXPECT_TRUE(clr_packing_problem_is_valid(&problem));
    EXPECT_U64(problem.board.width, 10);
    EXPECT_U64(problem.board.initial_mask, UINT64_C(0x3f0));
    EXPECT_U64(problem.piece_window.max_pieces, 5);
    EXPECT_U64(problem.piece_multiset_window.total_count, 3);
    EXPECT_U64(problem.piece_multiset_window.counts[CLR_PIECE_O], 1);
    EXPECT_U64(problem.piece_source.source_kind, CLR_PIECE_SOURCE_FIXED_QUEUE);
    EXPECT_U64(problem.rule.rule_profile_id, CLR_RULE_SRS_PLUS);
    EXPECT_U64(problem.rule.kick_profile_id, CLR_KICK_SRS_PLUS_180);
    EXPECT_U64(problem.backend.requested_backend, CLR_BACKEND_CPU);
    EXPECT_U64(problem.goal, CLR_GOAL_CLEAR_TO_EMPTY);
}static void packing_problem_masks_are_validated_against_search_height(void) {
    clr_packing_problem problem = valid_problem();
    uint64_t above_visible_inside_search = UINT64_C(1) << 25;

    problem.goal_region_mask = above_visible_inside_search;
    problem.required_fill_mask = above_visible_inside_search;
    problem.forbidden_mask = 0;

    EXPECT_TRUE(clr_packing_problem_is_valid(&problem));

    problem.goal_region_mask = UINT64_C(1) << 40;
    problem.required_fill_mask = problem.goal_region_mask;

    EXPECT_FALSE(clr_packing_problem_is_valid(&problem));
}static void buildup_problem_wraps_packing_problem(void) {
    clr_packing_problem packing = valid_problem();
    clr_buildup_problem buildup = clr_buildup_problem_from_packing(packing);

    EXPECT_U64(buildup.packing.board.width, 10);
    EXPECT_U64(buildup.packing.piece_multiset_window.total_count, 3);
    EXPECT_U64(buildup.piece_source.piece_source_id,
               packing.piece_source.piece_source_id);
    EXPECT_U64(buildup.buildup_flags, 0);
}int main(void) {
    compact_problem_preserves_search_problem_fields();
    packing_problem_masks_are_validated_against_search_height();
    buildup_problem_wraps_packing_problem();
    puts("core-c problem descriptor tests passed");
    return 0;
}
