#include "scheduler_tests_support.h"

#include <string.h>
void scheduler_test_set_board_descriptor(
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
}
ClearraBoard64Layout scheduler_test_two_line_layout(void) {
    ClearraBoard64Layout layout;
    if (clearra_board64_make_layout(10, 2, &layout) != CLEARRA_BOARD64_OK) {
        fprintf(stderr, "failed to create 10x2 layout\n");
        exit(1);
    }
    return layout;
}
void scheduler_test_scheduler_batch_into(ClearraGpuPackingBatchDescriptor *out_batch) {
    const uint8_t pieces[1] = {
        CLR_PIECE_O,
    };
    const uint64_t active_region_mask = (UINT64_C(1) << 20) - UINT64_C(1);
    const uint64_t o_missing_region = UINT64_C(0x0000000000000c03);
    EXPECT_GPU_STATUS(clearra_gpu_batch_descriptor_init(
                          scheduler_test_two_line_layout(),
                          active_region_mask & ~o_missing_region,
                          2,
                          pieces,
                          1,
                          out_batch),
                      CLEARRA_GPU_OK);
}ClearraGpuPackingBatchDescriptor scheduler_test_scheduler_batch(void) {
    ClearraGpuPackingBatchDescriptor batch;
    scheduler_test_scheduler_batch_into(&batch);
    return batch;
}
uint64_t scheduler_test_low_mask_for_cells(uint32_t cell_count) {
    if (cell_count >= 64u) {
        return UINT64_MAX;
    }
    return (UINT64_C(1) << cell_count) - UINT64_C(1);
}
void scheduler_test_scheduler_packing_problem_into(clr_packing_problem *out_problem) {
    clr_packing_problem problem = clr_packing_problem_zero();
    if (out_problem == 0) {
        fprintf(stderr, "missing scheduler test packing problem output\n");
        exit(1);
    }
    problem.problem_kind = CLR_PROBLEM_SCENARIO_PC;
    const uint64_t active_region_mask = scheduler_test_low_mask_for_cells(20);
    const uint64_t o_missing_region = UINT64_C(0x0000000000000c03);
    const uint64_t initial_mask = active_region_mask & ~o_missing_region;
    problem.max_pieces = 1;
    scheduler_test_set_board_descriptor(&problem.board, 10, 2, 2, initial_mask);
    problem.goal_region_mask = scheduler_test_low_mask_for_cells(
        (uint32_t)problem.board.width * problem.board.visible_height);
    problem.required_fill_mask = problem.goal_region_mask;
    problem.exact_pieces = 1;
    problem.piece_window.max_pieces = 1;
    problem.piece_window.exact_pieces = 1;
    problem.piece_window.has_exact_pieces = 1;
    uint8_t pieces[1] = {CLR_PIECE_O};
    problem.piece_multiset_window =
        clearra_piece_multiset_window_from_pieces(pieces, 1);
    problem.piece_source = clearra_piece_source_descriptor_fixed_queue(
        1u,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE,
        1,
        CLR_PIECE_SET_STANDARD_TETROMINOES);
    memcpy(problem.piece_source_pattern_pieces, pieces, 1);
    problem.piece_source_pattern_len = 1;
    problem.piece_source_pattern_complete = 1u;
    problem.piece_source_pattern_truncation_reason = 0u;
    problem.piece_source_pattern_id = 0u;
    problem.rule.piece_set_profile_id = CLR_PIECE_SET_STANDARD_TETROMINOES;
    problem.rule.bag_profile_id = CLR_BAG_STANDARD_7_BAG;
    problem.rule.rule_profile_id = CLR_RULE_NO_KICK;
    problem.rule.kick_profile_id = CLR_KICK_NO_KICK;
    problem.rule.spawn_profile_id = CLR_SPAWN_STANDARD_10;
    problem.backend.requested_backend = CLR_BACKEND_HYBRID;
    problem.backend.workers = 1;
    problem.backend.deterministic = 1;
    problem.backend.fallback_policy = CLR_BACKEND_FALLBACK_ALLOW;
    problem.goal = CLR_GOAL_CLEAR_TO_EMPTY;
    problem.count_policy = CLR_COUNT_ALL;
    problem.objective = CLR_OBJECTIVE_ALL;
    *out_problem = problem;
}clr_packing_problem scheduler_test_scheduler_packing_problem(void) {
    clr_packing_problem problem;
    scheduler_test_scheduler_packing_problem_into(&problem);
    return problem;
}