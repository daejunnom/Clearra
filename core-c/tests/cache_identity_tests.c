#include "../src/buildup/buildup_memo.h"
#include "../src/candidate/candidate.h"

#include <stdio.h>
#include <stdlib.h>

#define EXPECT_TRUE(EXPR)                                                       \
    do {                                                                        \
        if (!(EXPR)) {                                                          \
            fprintf(stderr, "%s:%d expected true\n", __FILE__, __LINE__);      \
            exit(1);                                                            \
        }                                                                       \
    } while (0)

#define EXPECT_FALSE(EXPR)                                                      \
    do {                                                                        \
        if ((EXPR)) {                                                           \
            fprintf(stderr, "%s:%d expected false\n", __FILE__, __LINE__);     \
            exit(1);                                                            \
        }                                                                       \
    } while (0)

#define EXPECT_NE_U64(LEFT, RIGHT)                                              \
    do {                                                                        \
        uint64_t left_value = (uint64_t)(LEFT);                                  \
        uint64_t right_value = (uint64_t)(RIGHT);                                \
        if (left_value == right_value) {                                         \
            fprintf(stderr, "%s:%d expected keys to differ but both were 0x%llx\n", \
                    __FILE__, __LINE__, (unsigned long long)left_value);         \
            exit(1);                                                            \
        }                                                                       \
    } while (0)
static ClearraCacheIdentity full_identity(void) {
    ClearraCacheIdentity identity = clearra_cache_identity_zero();
    identity.board = UINT64_C(0x1234);
    identity.piece_set_profile = UINT64_C(0x10);
    identity.piece_definition_id_fingerprint = UINT64_C(0x1010);
    identity.piece_area_multiset_fingerprint = UINT64_C(0x1011);
    identity.rule_kick_profile = UINT64_C(0x20);
    identity.backend_mode = 1;
    identity.operation_table_version = 7;
    identity.supply_provenance = UINT64_C(0x30);
    identity.queue_pattern_id = 4;
    identity.piece_window_start = 0;
    identity.piece_window_len = 5;
    identity.goal_id = UINT64_C(0x40);
    return identity;
}static clr_packing_problem descriptor_identity_problem(void) {
    clr_packing_problem problem = clr_packing_problem_zero();
    problem.problem_kind = CLR_PROBLEM_SCENARIO_PC;
    problem.max_pieces = 5;
    if (clr_board_descriptor_init(
            10, 2, 2, UINT64_C(0x3f0), 0, &problem.board) != CLR_BOARD_OK) {
        fprintf(stderr, "failed to initialize descriptor identity board\n");
        exit(1);
    }
    problem.goal_region_mask = (UINT64_C(1) << 20) - UINT64_C(1);
    problem.required_fill_mask = problem.goal_region_mask & ~problem.board.initial_mask;
    problem.piece_window.max_pieces = 5;
    problem.piece_window.exact_pieces = 5;
    problem.piece_window.has_exact_pieces = 1;
    problem.exact_pieces = 5;
    uint8_t pieces[] = {
        CLR_PIECE_I,
        CLR_PIECE_I,
        CLR_PIECE_O,
        CLR_PIECE_O,
        CLR_PIECE_O,
    };
    problem.piece_multiset_window =
        clearra_piece_multiset_window_from_pieces(pieces, 5);
    problem.piece_source = clearra_piece_source_descriptor_fixed_queue(
        1u,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE,
        5,
        CLR_PIECE_SET_STANDARD_TETROMINOES);
    problem.rule.piece_set_profile_id = CLR_PIECE_SET_STANDARD_TETROMINOES;
    problem.rule.bag_profile_id = CLR_BAG_STANDARD_7_BAG;
    problem.rule.rule_profile_id = CLR_RULE_SRS_PLUS;
    problem.rule.kick_profile_id = CLR_KICK_SRS_PLUS_180;
    problem.rule.spawn_profile_id = CLR_SPAWN_STANDARD_10;
    problem.backend.requested_backend = CLR_BACKEND_CPU;
    problem.goal = CLR_GOAL_CLEAR_TO_EMPTY;
    problem.count_policy = CLR_COUNT_ALL;
    problem.objective = CLR_OBJECTIVE_ALL;
    return problem;
}static void cache_identity_requires_source_or_queue_identity(void) {
    ClearraCacheIdentity identity = full_identity();
    EXPECT_TRUE(clearra_cache_identity_is_complete(identity));

    identity.supply_provenance = 0;
    identity.queue_pattern_id = 0;
    EXPECT_FALSE(clearra_cache_identity_is_complete(identity));
}static void cache_identity_requires_piece_definition_id_fingerprint(void) {
    ClearraCacheIdentity identity = full_identity();
    EXPECT_TRUE(clearra_cache_identity_is_complete(identity));

    identity.piece_definition_id_fingerprint = 0;
    EXPECT_FALSE(clearra_cache_identity_is_complete(identity));
}static void cache_identity_requires_piece_area_multiset_fingerprint(void) {
    ClearraCacheIdentity identity = full_identity();
    EXPECT_TRUE(clearra_cache_identity_is_complete(identity));

    identity.piece_area_multiset_fingerprint = 0;
    EXPECT_FALSE(clearra_cache_identity_is_complete(identity));
}static void different_piece_definition_ids_do_not_share_cache_key(void) {
    ClearraCacheIdentity left = full_identity();
    ClearraCacheIdentity right = full_identity();
    right.piece_definition_id_fingerprint = UINT64_C(0x2020);

    EXPECT_NE_U64(clearra_cache_identity_hash(left), clearra_cache_identity_hash(right));
}static void different_piece_area_multisets_do_not_share_cache_key(void) {
    ClearraCacheIdentity left = full_identity();
    ClearraCacheIdentity right = full_identity();
    right.piece_area_multiset_fingerprint = UINT64_C(0x3030);

    EXPECT_NE_U64(clearra_cache_identity_hash(left), clearra_cache_identity_hash(right));
}static void different_queue_pattern_ids_do_not_share_cache_key(void) {
    ClearraCacheIdentity left = full_identity();
    ClearraCacheIdentity right = full_identity();
    right.queue_pattern_id = 99;

    EXPECT_NE_U64(clearra_cache_identity_hash(left), clearra_cache_identity_hash(right));
}static void different_supply_provenance_does_not_share_cache_key(void) {
    ClearraCacheIdentity left = full_identity();
    ClearraCacheIdentity right = full_identity();
    right.supply_provenance = UINT64_C(0x31);

    EXPECT_NE_U64(clearra_cache_identity_hash(left), clearra_cache_identity_hash(right));
}static void different_rule_kick_profiles_do_not_share_cache_key(void) {
    ClearraCacheIdentity left = full_identity();
    ClearraCacheIdentity right = full_identity();
    right.rule_kick_profile = UINT64_C(0x21);

    EXPECT_NE_U64(clearra_candidate_cache_key(
                      left, CLEARRA_CANDIDATE_PIECE_T,
                      CLEARRA_CANDIDATE_MODE_LOCKED),
                  clearra_candidate_cache_key(
                      right, CLEARRA_CANDIDATE_PIECE_T,
                      CLEARRA_CANDIDATE_MODE_LOCKED));
}static void different_backend_modes_do_not_share_cache_key(void) {
    ClearraCacheIdentity left = full_identity();
    ClearraCacheIdentity right = full_identity();
    right.backend_mode = 2;

    EXPECT_NE_U64(clearra_cache_identity_hash(left), clearra_cache_identity_hash(right));
}static void different_operation_table_versions_do_not_share_cache_key(void) {
    ClearraCacheIdentity left = full_identity();
    ClearraCacheIdentity right = full_identity();
    right.operation_table_version = 8;

    EXPECT_NE_U64(clearra_cache_identity_hash(left), clearra_cache_identity_hash(right));
}static void different_piece_windows_do_not_share_cache_key(void) {
    ClearraCacheIdentity left = full_identity();
    ClearraCacheIdentity right = full_identity();
    right.piece_window_start = 2;
    right.piece_window_len = 6;

    EXPECT_NE_U64(clearra_cache_identity_hash(left), clearra_cache_identity_hash(right));
}static void different_goal_ids_do_not_share_cache_key(void) {
    ClearraCacheIdentity left = full_identity();
    ClearraCacheIdentity right = full_identity();
    right.goal_id = UINT64_C(0x41);

    EXPECT_NE_U64(clearra_cache_identity_hash(left), clearra_cache_identity_hash(right));
}static void candidate_and_buildup_memo_keys_are_separated(void) {
    ClearraCacheIdentity identity = full_identity();
    uint64_t candidate_key = clearra_candidate_cache_key(
        identity, CLEARRA_CANDIDATE_PIECE_T, CLEARRA_CANDIDATE_MODE_LOCKED);
    uint64_t buildup_key =
        clearra_buildup_memo_key(identity, UINT64_C(0x999), 3, 2, 1);

    EXPECT_NE_U64(candidate_key, buildup_key);
}static void cache_identity_includes_supply_rule_piece_goal(void) {
    clr_packing_problem base_problem = descriptor_identity_problem();
    ClearraCacheIdentity base =
        clearra_cache_identity_from_packing_problem(&base_problem, 7);
    EXPECT_TRUE(clearra_cache_identity_is_complete(base));
    EXPECT_TRUE(base.piece_definition_id_fingerprint != 0);
    EXPECT_TRUE(base.piece_area_multiset_fingerprint != 0);

    clr_packing_problem changed_supply = base_problem;
    uint8_t changed_pieces[] = {
        CLR_PIECE_I,
        CLR_PIECE_I,
        CLR_PIECE_O,
        CLR_PIECE_O,
        CLR_PIECE_T,
    };
    changed_supply.piece_multiset_window =
        clearra_piece_multiset_window_from_pieces(changed_pieces, 5);
    ClearraCacheIdentity supply_identity =
        clearra_cache_identity_from_packing_problem(&changed_supply, 7);
    EXPECT_NE_U64(clearra_cache_identity_hash(base),
                  clearra_cache_identity_hash(supply_identity));

    clr_packing_problem changed_rule = base_problem;
    changed_rule.rule.kick_profile_id = CLR_KICK_SRS_90;
    ClearraCacheIdentity rule_identity =
        clearra_cache_identity_from_packing_problem(&changed_rule, 7);
    EXPECT_NE_U64(clearra_cache_identity_hash(base),
                  clearra_cache_identity_hash(rule_identity));

    clr_packing_problem changed_piece = base_problem;
    changed_piece.rule.piece_set_profile_id = 99u;
    ClearraCacheIdentity piece_identity =
        clearra_cache_identity_from_packing_problem(&changed_piece, 7);
    EXPECT_NE_U64(clearra_cache_identity_hash(base),
                  clearra_cache_identity_hash(piece_identity));

    clr_packing_problem changed_goal = base_problem;
    changed_goal.piece_window.exact_pieces = 4;
    changed_goal.exact_pieces = 4;
    ClearraCacheIdentity goal_identity =
        clearra_cache_identity_from_packing_problem(&changed_goal, 7);
    EXPECT_NE_U64(clearra_cache_identity_hash(base),
                  clearra_cache_identity_hash(goal_identity));
}int main(void) {
    cache_identity_requires_source_or_queue_identity();
    cache_identity_requires_piece_definition_id_fingerprint();
    cache_identity_requires_piece_area_multiset_fingerprint();
    different_piece_definition_ids_do_not_share_cache_key();
    different_piece_area_multisets_do_not_share_cache_key();
    different_queue_pattern_ids_do_not_share_cache_key();
    different_supply_provenance_does_not_share_cache_key();
    different_rule_kick_profiles_do_not_share_cache_key();
    different_backend_modes_do_not_share_cache_key();
    different_operation_table_versions_do_not_share_cache_key();
    different_piece_windows_do_not_share_cache_key();
    different_goal_ids_do_not_share_cache_key();
    candidate_and_buildup_memo_keys_are_separated();
    cache_identity_includes_supply_rule_piece_goal();
    puts("core-c cache identity tests passed");
    return 0;
}
