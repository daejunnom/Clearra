#include "candidate_tests_support.h"
void candidate_cache_key_is_mode_scoped(void) {
    ClearraCandidateCacheEntry entry;
    ClearraCacheIdentity identity = candidate_test_full_cache_identity();
    uint64_t harddrop_key = clearra_candidate_cache_key(
        identity, CLEARRA_CANDIDATE_PIECE_T, CLEARRA_CANDIDATE_MODE_HARDDROP);
    uint64_t locked_key = clearra_candidate_cache_key(
        identity, CLEARRA_CANDIDATE_PIECE_T, CLEARRA_CANDIDATE_MODE_LOCKED);
    ClearraCacheIdentity other_queue = identity;
    other_queue.queue_pattern_id = 77;
    uint64_t other_queue_key = clearra_candidate_cache_key(
        other_queue, CLEARRA_CANDIDATE_PIECE_T, CLEARRA_CANDIDATE_MODE_HARDDROP);

    clearra_candidate_cache_entry_clear(&entry);
    clearra_candidate_cache_entry_store(&entry, harddrop_key, 34);
    EXPECT_TRUE(clearra_candidate_cache_entry_matches(entry, harddrop_key));
    EXPECT_FALSE(clearra_candidate_cache_entry_matches(entry, locked_key));
    EXPECT_FALSE(clearra_candidate_cache_entry_matches(entry, other_queue_key));
    EXPECT_U64(entry.count, 34);
}
void candidate_cache_key_includes_board_rule_piece(void) {
    ClearraCacheIdentity identity = candidate_test_full_cache_identity();
    uint64_t base = clearra_candidate_cache_key(
        identity, CLEARRA_CANDIDATE_PIECE_T, CLEARRA_CANDIDATE_MODE_LOCKED);

    ClearraCacheIdentity other_board = identity;
    other_board.board = UINT64_C(0x0807);
    uint64_t board_key = clearra_candidate_cache_key(
        other_board, CLEARRA_CANDIDATE_PIECE_T, CLEARRA_CANDIDATE_MODE_LOCKED);

    ClearraCacheIdentity other_rule = identity;
    other_rule.rule_kick_profile = 99;
    uint64_t rule_key = clearra_candidate_cache_key(
        other_rule, CLEARRA_CANDIDATE_PIECE_T, CLEARRA_CANDIDATE_MODE_LOCKED);

    uint64_t piece_key = clearra_candidate_cache_key(
        identity, CLEARRA_CANDIDATE_PIECE_L, CLEARRA_CANDIDATE_MODE_LOCKED);

    EXPECT_FALSE(base == board_key);
    EXPECT_FALSE(base == rule_key);
    EXPECT_FALSE(base == piece_key);
}
void duplicate_candidate_removed(void) {
    ClearraCandidateList candidates;
    ClearraCandidateOperation operation;

    clearra_candidate_list_clear(&candidates);
    operation.piece = CLEARRA_CANDIDATE_PIECE_T;
    operation.rotation = CLEARRA_CANDIDATE_ROTATION_ZERO;
    operation.x = 0;
    operation.y = 0;
    operation.mask = UINT64_C(0x0807);
    operation.transition_kind = CLEARRA_ROTATION_TRANSITION_NONE;
    operation.kick_index = 0;
    operation.kick_dx = 0;
    operation.kick_dy = 0;

    EXPECT_STATUS(clearra_candidate_push_operation(&candidates, operation),
                  CLEARRA_CANDIDATE_OK);
    operation.transition_kind = CLEARRA_ROTATION_TRANSITION_HALF_TURN;
    operation.kick_index = 3;
    EXPECT_STATUS(clearra_candidate_push_operation(&candidates, operation),
                  CLEARRA_CANDIDATE_OK);
    EXPECT_U64(candidates.count, 1);
}
