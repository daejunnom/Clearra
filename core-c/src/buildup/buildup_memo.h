#ifndef CLEARRA_BUILDUP_MEMO_H
#define CLEARRA_BUILDUP_MEMO_H

#include "../cache/cache_identity.h"
#include "buildup_bfs_state.h"

#include <stdbool.h>
#include <stdint.h>

typedef struct clr_buildup_memo_key {
    uint64_t cache_identity_hash;
    uint16_t remaining_ops_bitset;
    uint64_t current_board_mask;
    clr_deleted_line_state deleted_line_state;
    clr_buildup_hold_automaton_memo_key hold_automaton_state;
    uint16_t piece_source_cursor;
    uint64_t reachability_relevant_state;
    uint8_t cleared_lines;
    uint8_t reserved[5];
} clr_buildup_memo_key;
uint64_t clearra_buildup_memo_key(
    ClearraCacheIdentity identity,
    uint64_t remaining_operations,
    uint8_t hold_piece,
    uint16_t queue_cursor,
    uint16_t line_clear_state);
clr_buildup_memo_key clearra_buildup_memo_key_from_bfs_state(
    ClearraCacheIdentity identity,
    const clr_buildup_bfs_state *state);
clr_buildup_memo_key clearra_buildup_memo_key_from_bfs_state_hash(
    uint64_t cache_identity_hash,
    const clr_buildup_bfs_state *state);
uint64_t clearra_buildup_memo_key_hash(const clr_buildup_memo_key *key);
bool clearra_buildup_memo_key_equals_exact(
    const clr_buildup_memo_key *left,
    const clr_buildup_memo_key *right);
bool clearra_buildup_memo_key_matches_bucket(
    const clr_buildup_memo_key *left,
    uint64_t left_hash,
    const clr_buildup_memo_key *right,
    uint64_t right_hash);
#endif
