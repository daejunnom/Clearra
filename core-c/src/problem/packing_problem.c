#include "clr_problem.h"
clr_packing_problem clr_packing_problem_zero(void) {
    clr_packing_problem problem = {0};
    problem.budget = clr_problem_budget_zero();
    problem.checkpoint = clr_checkpoint_spec_none();
    return problem;
}static uint64_t low_mask_for(uint32_t bit_count) {
    if (bit_count == 0u) {
        return UINT64_C(0);
    }
    if (bit_count >= 64u) {
        return UINT64_MAX;
    }
    return (UINT64_C(1) << bit_count) - UINT64_C(1);
}bool clr_packing_problem_is_valid(const clr_packing_problem *problem) {
    if (problem == 0) {
        return false;
    }
    if (problem->problem_kind < CLR_PROBLEM_OPENING_PC ||
        problem->problem_kind > CLR_PROBLEM_BUILD) {
        return false;
    }
    if (!clr_board_descriptor_is_valid(&problem->board)) {
        return false;
    }
    if (problem->piece_window.max_pieces == 0 ||
        problem->max_pieces != problem->piece_window.max_pieces) {
        return false;
    }
    if (problem->piece_window.has_exact_pieces &&
        problem->exact_pieces != problem->piece_window.exact_pieces) {
        return false;
    }
    if (problem->goal_region_mask == 0u) {
        return false;
    }
    uint32_t search_cells =
        (uint32_t)problem->board.width * (uint32_t)problem->board.search_height;
    uint64_t search_mask = low_mask_for(search_cells);
    if ((problem->goal_region_mask & ~search_mask) != 0u ||
        (problem->required_fill_mask & ~problem->goal_region_mask) != 0u ||
        (problem->forbidden_mask & problem->goal_region_mask) != 0u ||
        (problem->forbidden_mask & ~search_mask) != 0u ||
        (problem->board.initial_mask & ~search_mask) != 0u ||
        (problem->board.initial_mask & problem->forbidden_mask) != 0u) {
        return false;
    }
    if (!clearra_piece_source_descriptor_valid(&problem->piece_source)) {
        return false;
    }
    bool incomplete_empty_multiset =
        problem->piece_multiset_window.total_count == 0u &&
        !clearra_piece_source_descriptor_is_complete(&problem->piece_source);
    if (!incomplete_empty_multiset &&
        !clearra_piece_multiset_window_is_valid(&problem->piece_multiset_window)) {
        return false;
    }
    if (!incomplete_empty_multiset &&
        !clearra_piece_multiset_family_is_valid(
            &problem->piece_multiset_family,
            &problem->piece_multiset_window)) {
        return false;
    }
    if (problem->piece_window.has_exact_pieces &&
        problem->piece_multiset_window.exact_count != 0u &&
        problem->piece_multiset_window.exact_count !=
            problem->piece_window.exact_pieces &&
        problem->piece_multiset_window.total_count >=
            problem->piece_window.exact_pieces) {
        return false;
    }
    if (problem->goal != CLR_GOAL_CLEAR_TO_EMPTY) {
        return false;
    }
    if (problem->rule.rule_profile_id == 0 || problem->rule.kick_profile_id == 0) {
        return false;
    }
    return true;
}
