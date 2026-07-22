#include "packing_tests_support.h"
void exact_piece_window_short_queue_is_empty_result_not_invalid(void) {
    static ClearraPackingCandidateBuffer buffer;
    ClearraBoard64Layout layout = packing_test_standard_two_line_layout();
    uint64_t target_mask = 0;
    EXPECT_STATUS(clearra_packing_target_mask_for_lines(layout, 2, &target_mask),
                  CLEARRA_PACKING_OK);
    clr_packing_problem problem = packing_test_short_queue_problem(layout, target_mask, true);

    EXPECT_STATUS(clearra_packing_enumerator_cpu_generate_problem(&problem, &buffer),
                  CLEARRA_PACKING_OK);

    EXPECT_U64(buffer.count, 0);
}
void non_exact_piece_window_clamps_to_available_queue(void) {
    static ClearraPackingCandidateBuffer buffer;
    ClearraBoard64Layout layout = packing_test_standard_two_line_layout();
    uint64_t target_mask = 0;
    EXPECT_STATUS(clearra_packing_target_mask_for_lines(layout, 2, &target_mask),
                  CLEARRA_PACKING_OK);
    clr_packing_problem problem = packing_test_short_queue_problem(layout, target_mask, false);

    EXPECT_STATUS(clearra_packing_enumerator_cpu_generate_problem(&problem, &buffer),
                  CLEARRA_PACKING_OK);

    EXPECT_U64(buffer.count, 0);
}