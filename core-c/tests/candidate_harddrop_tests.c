#include "candidate_tests_support.h"
void harddrop_candidate_matches_fixture(void) {
    ClearraCandidateList candidates;
    ClearraBoard64Layout layout = candidate_test_standard_10x4();

    EXPECT_STATUS(clearra_harddrop_candidates_generate(
                      layout, clearra_board64_empty(), CLEARRA_CANDIDATE_PIECE_T,
                      &candidates),
                  CLEARRA_CANDIDATE_OK);
    EXPECT_U64(candidates.count, 34);
    EXPECT_U64(candidates.operations[0].piece, CLEARRA_CANDIDATE_PIECE_T);
    EXPECT_U64(candidates.operations[0].rotation, CLEARRA_CANDIDATE_ROTATION_ZERO);
    EXPECT_I8(candidates.operations[0].x, 0);
    EXPECT_I8(candidates.operations[0].y, 0);
    EXPECT_U64(candidates.operations[0].mask, UINT64_C(0x0807));
}
void harddrop_candidate_count_fixture(void) {
    harddrop_candidate_matches_fixture();
}
void harddrop_candidate_rejects_blocked_fall_path(void) {
    ClearraCandidateList candidates;
    ClearraBoard64Layout layout = candidate_test_standard_10x4();
    uint64_t board = candidate_test_cell_mask(layout, 0, 2);

    EXPECT_STATUS(clearra_harddrop_candidates_generate(
                      layout, board, CLEARRA_CANDIDATE_PIECE_O, &candidates),
                  CLEARRA_CANDIDATE_OK);
    EXPECT_FALSE(candidate_test_candidate_list_contains(
        &candidates, CLEARRA_CANDIDATE_PIECE_O, CLEARRA_CANDIDATE_ROTATION_ZERO, 0, 0));
}
void harddrop_o_piece_uses_canonical_rotation_fixture(void) {
    ClearraCandidateList candidates;
    ClearraBoard64Layout layout = candidate_test_standard_10x4();

    EXPECT_STATUS(clearra_harddrop_candidates_generate(
                      layout, clearra_board64_empty(), CLEARRA_CANDIDATE_PIECE_O,
                      &candidates),
                  CLEARRA_CANDIDATE_OK);
    EXPECT_U64(candidates.count, 9);
    for (uint16_t index = 0; index < candidates.count; index++) {
        EXPECT_U64(candidates.operations[index].rotation,
                   CLEARRA_CANDIDATE_ROTATION_ZERO);
    }
}