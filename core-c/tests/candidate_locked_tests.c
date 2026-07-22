#include "candidate_tests_support.h"
void locked_candidate_matches_fixture(void) {
    ClearraCandidateList candidates;
    ClearraCandidateList harddrop_candidates;
    ClearraBoard64Layout layout = candidate_test_standard_10x4();
    ClearraCompactRuleProfile rule = candidate_test_compact_rule(CLR_RULE_SRS, CLR_KICK_SRS_90);

    EXPECT_STATUS(clearra_candidate_search(
                      layout, clearra_board64_empty(), CLEARRA_CANDIDATE_PIECE_T, &rule,
                      CLEARRA_CANDIDATE_MODE_LOCKED, &candidates),
                  CLEARRA_CANDIDATE_OK);
    EXPECT_STATUS(clearra_harddrop_candidates_generate(
                      layout, clearra_board64_empty(), CLEARRA_CANDIDATE_PIECE_T,
                      &harddrop_candidates),
                  CLEARRA_CANDIDATE_OK);
    EXPECT_U64(candidates.count, harddrop_candidates.count);
    EXPECT_TRUE(candidate_test_candidate_list_contains(
        &candidates, CLEARRA_CANDIDATE_PIECE_T, CLEARRA_CANDIDATE_ROTATION_ZERO, 0, 0));
}
void locked_candidate_count_fixture(void) {
    locked_candidate_matches_fixture();
}
void locked_candidate_uses_reverse_graph_not_harddrop_alias(void) {
    ClearraCandidateList harddrop_candidates;
    ClearraCandidateList locked_candidates;
    ClearraBoard64Layout layout = candidate_test_standard_10x4();
    ClearraCompactRuleProfile rule = candidate_test_compact_rule(CLR_RULE_SRS, CLR_KICK_SRS_90);
    uint64_t board = candidate_test_cell_mask(layout, 0, 2);

    EXPECT_STATUS(clearra_harddrop_candidates_generate(
                      layout, board, CLEARRA_CANDIDATE_PIECE_O, &harddrop_candidates),
                  CLEARRA_CANDIDATE_OK);
    EXPECT_FALSE(candidate_test_candidate_list_contains(
        &harddrop_candidates, CLEARRA_CANDIDATE_PIECE_O,
        CLEARRA_CANDIDATE_ROTATION_ZERO, 0, 0));

    EXPECT_STATUS(clearra_candidate_search(
                      layout, board, CLEARRA_CANDIDATE_PIECE_O, &rule,
                      CLEARRA_CANDIDATE_MODE_LOCKED, &locked_candidates),
                  CLEARRA_CANDIDATE_OK);
    EXPECT_TRUE(candidate_test_candidate_list_contains(
        &locked_candidates, CLEARRA_CANDIDATE_PIECE_O,
        CLEARRA_CANDIDATE_ROTATION_ZERO, 0, 0));
}
void locked_candidate_rejects_collision_free_unreachable_placement(void) {
    ClearraCandidateList locked_candidates;
    ClearraBoard64Layout layout = candidate_test_standard_10x4();
    ClearraCompactRuleProfile rule = candidate_test_compact_rule(CLR_RULE_SRS, CLR_KICK_SRS_90);
    uint64_t mask = 0;
    bool collision = true;

    EXPECT_STATUS(clearra_candidate_mask_for_piece(
                      layout, CLEARRA_CANDIDATE_PIECE_O,
                      CLEARRA_CANDIDATE_ROTATION_ZERO, 4, 2, &mask),
                  CLEARRA_CANDIDATE_OK);
    EXPECT_BOARD_STATUS(clearra_board64_collision(
                            layout, clearra_board64_empty(), mask, &collision),
                        CLEARRA_BOARD64_OK);
    EXPECT_FALSE(collision);

    EXPECT_STATUS(clearra_candidate_search(
                      layout, clearra_board64_empty(), CLEARRA_CANDIDATE_PIECE_O,
                      &rule, CLEARRA_CANDIDATE_MODE_LOCKED, &locked_candidates),
                  CLEARRA_CANDIDATE_OK);
    EXPECT_FALSE(candidate_test_candidate_list_contains(
        &locked_candidates, CLEARRA_CANDIDATE_PIECE_O,
        CLEARRA_CANDIDATE_ROTATION_ZERO, 4, 2));
}
void locked180_candidate_matches_fixture(void) {
    ClearraCandidateList candidates;
    ClearraBoard64Layout layout = candidate_test_standard_10x4();
    ClearraCompactRuleProfile rule =
        candidate_test_compact_rule(CLR_RULE_SRS_PLUS, CLR_KICK_SRS_PLUS_180);

    EXPECT_STATUS(clearra_candidate_search(
                      layout, clearra_board64_empty(), CLEARRA_CANDIDATE_PIECE_T, &rule,
                      CLEARRA_CANDIDATE_MODE_LOCKED_180, &candidates),
                  CLEARRA_CANDIDATE_OK);
    EXPECT_U64(candidates.count, 34);
    EXPECT_TRUE(rule.supports_180);
    EXPECT_TRUE(candidate_test_candidate_list_has_transition(
        &candidates, CLEARRA_ROTATION_TRANSITION_HALF_TURN));
}
void locked180_candidate_count_fixture(void) {
    locked180_candidate_matches_fixture();
}
void srs_plus_i_half_turn_displacements_match_tetrio_fixture(void) {
    static const uint8_t from_rotations[4] = {
        CLEARRA_CANDIDATE_ROTATION_ZERO,
        CLEARRA_CANDIDATE_ROTATION_RIGHT,
        CLEARRA_CANDIDATE_ROTATION_TWO,
        CLEARRA_CANDIDATE_ROTATION_LEFT,
    };
    static const uint8_t to_rotations[4] = {
        CLEARRA_CANDIDATE_ROTATION_TWO,
        CLEARRA_CANDIDATE_ROTATION_LEFT,
        CLEARRA_CANDIDATE_ROTATION_ZERO,
        CLEARRA_CANDIDATE_ROTATION_RIGHT,
    };
    static const int8_t expected[4][6][2] = {
        {{0, -1}, {0, 0}, {1, 0}, {-1, 0}, {1, -1}, {-1, -1}},
        {{0, 1}, {0, 0}, {-1, 0}, {1, 0}, {-1, 1}, {1, 1}},
        {{-1, 0}, {0, 0}, {0, 2}, {0, 1}, {-1, 2}, {-1, 1}},
        {{1, 0}, {0, 0}, {0, 2}, {0, 1}, {1, 2}, {1, 1}},
    };
    ClearraCompactRuleProfile rule =
        candidate_test_compact_rule(CLR_RULE_SRS_PLUS, CLR_KICK_SRS_PLUS_180);

    for (uint8_t transition = 0; transition < 4; transition++) {
        const ClearraCompactKickSequence *sequence = 0;
        EXPECT_U64(
            clearra_kick_table_sequence_for(
                &rule.kick_table,
                CLR_PIECE_I,
                from_rotations[transition],
                to_rotations[transition],
                &sequence),
            CLEARRA_RULE_OK);
        EXPECT_U64(sequence->count, 6);
        for (uint8_t kick_index = 0; kick_index < sequence->count; kick_index++) {
            int8_t normalized_dx = 0;
            int8_t normalized_dy = 0;
            EXPECT_STATUS(
                clearra_candidate_normalized_kick_delta(
                    CLR_PIECE_I,
                    from_rotations[transition],
                    to_rotations[transition],
                    sequence->offsets[kick_index].dx,
                    sequence->offsets[kick_index].dy,
                    &normalized_dx,
                    &normalized_dy),
                CLEARRA_CANDIDATE_OK);
            EXPECT_I8(normalized_dx, expected[transition][kick_index][0]);
            EXPECT_I8(normalized_dy, expected[transition][kick_index][1]);
        }
    }
}
void locked180_candidate_finds_half_turn_only_placement(void) {
    ClearraCandidateList locked_candidates;
    ClearraCandidateList locked180_candidates;
    ClearraBoard64Layout layout = candidate_test_standard_10x4();
    ClearraReachabilityKickTable table;
    candidate_test_candidate_fixture_kick_table(&table);
    uint64_t board = candidate_test_half_turn_only_board(layout);

    EXPECT_STATUS(clearra_locked_candidates_generate_with_kicks(
                      layout, board, CLEARRA_CANDIDATE_PIECE_T, &table,
                      &locked_candidates),
                  CLEARRA_CANDIDATE_OK);
    EXPECT_STATUS(clearra_locked180_candidates_generate_with_kicks(
                      layout, board, CLEARRA_CANDIDATE_PIECE_T, &table,
                      &locked180_candidates),
                  CLEARRA_CANDIDATE_OK);
    EXPECT_FALSE(candidate_test_candidate_list_contains(
        &locked_candidates, CLEARRA_CANDIDATE_PIECE_T,
        CLEARRA_CANDIDATE_ROTATION_TWO, 6, 0));
    EXPECT_TRUE(candidate_test_candidate_list_contains_with_transition(
        &locked180_candidates, CLEARRA_CANDIDATE_PIECE_T,
        CLEARRA_CANDIDATE_ROTATION_TWO, 6, 0,
        CLEARRA_ROTATION_TRANSITION_HALF_TURN));
}
void unreachable_placement_reject_fixture(void) {
    ClearraBoard64Layout layout = candidate_test_standard_10x4();
    bool reachable = true;

    EXPECT_STATUS(clearra_candidate_is_reachable_operation(
                      layout, clearra_board64_empty(), CLEARRA_CANDIDATE_PIECE_O,
                      CLEARRA_CANDIDATE_ROTATION_ZERO, 4, 2, &reachable),
                  CLEARRA_CANDIDATE_OK);
    EXPECT_FALSE(reachable);
}
