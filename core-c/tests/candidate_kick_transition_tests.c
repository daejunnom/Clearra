#include "candidate_tests_support.h"
void kick_first_success_ordering_fixture(void) {
    ClearraBoard64Layout layout = candidate_test_standard_10x4();
    ClearraKickOffset offsets[2] = {{-1, 1}, {1, 1}};
    ClearraCandidateOperation operation;
    uint64_t colliding_mask = 0;

    EXPECT_STATUS(clearra_candidate_mask_for_piece(
                      layout, CLEARRA_CANDIDATE_PIECE_T,
                      CLEARRA_CANDIDATE_ROTATION_RIGHT, 0, 0, &colliding_mask),
                  CLEARRA_CANDIDATE_OK);
    EXPECT_STATUS(clearra_candidate_first_success_kick(
                      layout, colliding_mask, CLEARRA_CANDIDATE_PIECE_T,
                      CLEARRA_CANDIDATE_ROTATION_ZERO, CLEARRA_CANDIDATE_ROTATION_RIGHT,
                      0, 0, offsets, 2, &operation),
                  CLEARRA_CANDIDATE_OK);

    EXPECT_U64(operation.kick_index, 1);
    EXPECT_U64(operation.transition_kind, CLEARRA_ROTATION_TRANSITION_CLOCKWISE);
    EXPECT_U64(operation.x, 2);
}
void kick_first_success_prefers_earliest_valid_offset_fixture(void) {
    ClearraBoard64Layout layout = candidate_test_standard_10x4();
    ClearraKickOffset offsets[2] = {{0, 1}, {1, 1}};
    ClearraCandidateOperation operation;

    EXPECT_STATUS(clearra_candidate_first_success_kick(
                      layout, clearra_board64_empty(), CLEARRA_CANDIDATE_PIECE_T,
                      CLEARRA_CANDIDATE_ROTATION_ZERO, CLEARRA_CANDIDATE_ROTATION_RIGHT,
                      0, 0, offsets, 2, &operation),
                  CLEARRA_CANDIDATE_OK);

    EXPECT_U64(operation.kick_index, 0);
    EXPECT_U64(operation.transition_kind, CLEARRA_ROTATION_TRANSITION_CLOCKWISE);
    EXPECT_U64(operation.x, 1);
}
void rotation_transition_correctness_fixture(void) {
    ClearraRotationTransitionKind transition = CLEARRA_ROTATION_TRANSITION_NONE;

    EXPECT_STATUS(clearra_candidate_transition_kind(
                      CLEARRA_CANDIDATE_ROTATION_ZERO,
                      CLEARRA_CANDIDATE_ROTATION_RIGHT, &transition),
                  CLEARRA_CANDIDATE_OK);
    EXPECT_U64(transition, CLEARRA_ROTATION_TRANSITION_CLOCKWISE);

    EXPECT_STATUS(clearra_candidate_transition_kind(
                      CLEARRA_CANDIDATE_ROTATION_RIGHT,
                      CLEARRA_CANDIDATE_ROTATION_ZERO, &transition),
                  CLEARRA_CANDIDATE_OK);
    EXPECT_U64(transition, CLEARRA_ROTATION_TRANSITION_COUNTER_CLOCKWISE);

    EXPECT_STATUS(clearra_candidate_transition_kind(
                      CLEARRA_CANDIDATE_ROTATION_ZERO,
                      CLEARRA_CANDIDATE_ROTATION_TWO, &transition),
                  CLEARRA_CANDIDATE_OK);
    EXPECT_U64(transition, CLEARRA_ROTATION_TRANSITION_HALF_TURN);

    EXPECT_STATUS(clearra_candidate_transition_kind(
                      CLEARRA_CANDIDATE_ROTATION_TWO,
                      CLEARRA_CANDIDATE_ROTATION_TWO, &transition),
                  CLEARRA_CANDIDATE_OK);
    EXPECT_U64(transition, CLEARRA_ROTATION_TRANSITION_NONE);
}
