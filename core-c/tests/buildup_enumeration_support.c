#include "buildup_tests_support.h"

clr_buildup_problem buildup_test_two_operation_gap_fill_problem(void) {
    ClearraBoard64Layout layout = buildup_test_standard_10x2_layout();
    uint8_t columns[2] = {0, 2};
    clr_packing_problem packing = buildup_test_buildup_packing_problem(2, 2, 0);
    uint64_t target_mask = buildup_test_low_mask_for_cells(20);
    uint64_t first_o = buildup_test_o_mask_at(layout, 0, 0);
    uint64_t second_o = buildup_test_o_mask_at(layout, 2, 0);
    packing.board.initial_mask = target_mask & ~(first_o | second_o);
    packing.board.cell_count = (uint32_t)packing.board.width *
                               (uint32_t)packing.board.search_height;
    packing.required_fill_mask = first_o | second_o;
    const uint8_t pieces[2] = {CLR_PIECE_O, CLR_PIECE_O};
    buildup_test_set_packing_pieces(
        &packing,
        pieces,
        2,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);

    return buildup_test_build_problem_from_candidate(
        packing, buildup_test_o_candidate_for_columns(layout, columns, 2));
}
