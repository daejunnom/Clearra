#include "candidate_tests_support.h"
ClearraBoard64Layout candidate_test_standard_10x4(void) {
    ClearraBoard64Layout layout;
    EXPECT_BOARD_STATUS(clearra_board64_make_layout(10, 4, &layout), CLEARRA_BOARD64_OK);
    return layout;
}
ClearraCacheIdentity candidate_test_full_cache_identity(void) {
    ClearraCacheIdentity identity = clearra_cache_identity_zero();
    identity.board = clearra_board64_empty();
    identity.piece_set_profile = 1;
    identity.piece_definition_id_fingerprint = 11;
    identity.piece_area_multiset_fingerprint = 12;
    identity.rule_kick_profile = 2;
    identity.backend_mode = 3;
    identity.operation_table_version = 4;
    identity.supply_provenance = 5;
    identity.queue_pattern_id = 6;
    identity.piece_window_start = 0;
    identity.piece_window_len = 5;
    identity.goal_id = 7;
    return identity;
}
clr_rule_profile_descriptor candidate_test_rule_descriptor(uint32_t rule, uint32_t kick) {
    clr_rule_profile_descriptor descriptor = {0};
    descriptor.piece_set_profile_id = CLR_PIECE_SET_STANDARD_TETROMINOES;
    descriptor.bag_profile_id = CLR_BAG_STANDARD_7_BAG;
    descriptor.rule_profile_id = rule;
    descriptor.kick_profile_id = kick;
    descriptor.spawn_profile_id = CLR_SPAWN_STANDARD_10;
    descriptor.has_verified_kick_profile = 0;
    return descriptor;
}
ClearraCompactRuleProfile candidate_test_compact_rule(uint32_t rule, uint32_t kick) {
    ClearraCompactRuleProfile profile;
    clr_rule_profile_descriptor descriptor = candidate_test_rule_descriptor(rule, kick);
    ClearraRuleStatus status = clearra_rule_profile_from_descriptor(&descriptor, &profile);
    if (status != CLEARRA_RULE_OK) {
        fprintf(stderr, "%s:%d expected compact rule status ok but got %d\n", __FILE__,
                __LINE__, (int)status);
        exit(1);
    }
    return profile;
}

uint64_t candidate_test_cell_mask(ClearraBoard64Layout layout, uint8_t x, uint8_t y) {
    uint8_t index = 0;
    EXPECT_BOARD_STATUS(clearra_board64_cell_index(layout, x, y, &index),
                        CLEARRA_BOARD64_OK);
    return UINT64_C(1) << index;
}
uint64_t candidate_test_half_turn_only_board(ClearraBoard64Layout layout) {
    (void)layout;
    return UINT64_C(0x8099802143);
}
void candidate_test_candidate_fixture_kick_table(
    ClearraReachabilityKickTable *out_table) {
    *out_table = (ClearraReachabilityKickTable){0};
    EXPECT_TRUE(clearra_srs_plus_kick_table(&out_table->owned_compact_table) ==
                CLEARRA_RULE_OK);
    out_table->compact_table = &out_table->owned_compact_table;
    out_table->piece = CLEARRA_CANDIDATE_PIECE_T;
}
bool candidate_test_candidate_list_contains(
    const ClearraCandidateList *list,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y) {
    for (uint16_t index = 0; index < list->count; index++) {
        const ClearraCandidateOperation *operation = &list->operations[index];
        if (operation->piece == piece && operation->rotation == rotation &&
            operation->x == x && operation->y == y) {
            return true;
        }
    }
    return false;
}
bool candidate_test_candidate_list_has_transition(
    const ClearraCandidateList *list,
    uint8_t transition_kind) {
    for (uint16_t index = 0; index < list->count; index++) {
        if (list->operations[index].transition_kind == transition_kind) {
            return true;
        }
    }
    return false;
}
bool candidate_test_candidate_list_contains_with_transition(
    const ClearraCandidateList *list,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    uint8_t transition_kind) {
    for (uint16_t index = 0; index < list->count; index++) {
        const ClearraCandidateOperation *operation = &list->operations[index];
        if (operation->piece == piece && operation->rotation == rotation &&
            operation->x == x && operation->y == y &&
            operation->transition_kind == transition_kind) {
            return true;
        }
    }
    return false;
}
