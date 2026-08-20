#include "buildup_memo.h"
#include "buildup_state.h"

uint64_t clearra_buildup_memo_key(
    ClearraCacheIdentity identity,
    uint64_t remaining_operations,
    uint8_t hold_piece,
    uint16_t queue_cursor,
    uint16_t line_clear_state) {
    uint64_t hash = clearra_cache_identity_hash(identity);
    hash = clearra_cache_key_mix_u64(hash, UINT64_C(0x4255494c44555000));
    hash = clearra_cache_key_mix_u64(hash, remaining_operations);
    hash = clearra_cache_key_mix_u64(hash, hold_piece);
    hash = clearra_cache_key_mix_u64(hash, queue_cursor);
    hash = clearra_cache_key_mix_u64(hash, line_clear_state);
    return hash;
}

static uint64_t mix_deleted_line_state(
    uint64_t hash,
    clr_deleted_line_state state) {
    hash = clearra_cache_key_mix_u64(hash, state.deleted_row_mask);
    hash = clearra_cache_key_mix_u64(hash, state.deleted_count);
    return hash;
}

clr_buildup_memo_key clearra_buildup_memo_key_from_bfs_state_hash(
    uint64_t cache_identity_hash,
    const clr_buildup_bfs_state *state) {
    clr_buildup_memo_key key = {0};
    key.cache_identity_hash = cache_identity_hash;
    if (state == 0) {
        return key;
    }

    clr_buildup_hold_automaton_memo_key hold_key =
        clearra_buildup_hold_automaton_memo_key(&state->hold_automaton_state);
    key.remaining_ops_bitset = state->remaining_ops_bitset;
    key.current_board_mask = state->current_board_mask;
    key.deleted_line_state = state->deleted_line_state;
    key.hold_automaton_state = hold_key;
    key.piece_source_cursor = state->piece_source_cursor;
    key.reachability_relevant_state = state->reachability_relevant_state;
    key.cleared_lines = state->cleared_lines;
    return key;
}

clr_buildup_memo_key clearra_buildup_memo_key_from_bfs_state(
    ClearraCacheIdentity identity,
    const clr_buildup_bfs_state *state) {
    return clearra_buildup_memo_key_from_bfs_state_hash(
        clearra_cache_identity_hash(identity), state);
}

uint64_t clearra_buildup_memo_key_hash(const clr_buildup_memo_key *key) {
    if (key == 0) {
        return 0u;
    }

    uint64_t hash = clearra_cache_key_mix_u64(
        key->cache_identity_hash, UINT64_C(0x42554653464b4559));
    hash = clearra_cache_key_mix_u64(hash, key->remaining_ops_bitset);
    hash = clearra_cache_key_mix_u64(hash, key->current_board_mask);
    hash = mix_deleted_line_state(hash, key->deleted_line_state);
    hash = clearra_cache_key_mix_u64(
        hash,
        clearra_buildup_hold_automaton_memo_key_hash(
            &key->hold_automaton_state));
    hash = clearra_cache_key_mix_u64(hash, key->piece_source_cursor);
    hash = clearra_cache_key_mix_u64(hash, key->reachability_relevant_state);
    hash = clearra_cache_key_mix_u64(hash, key->cleared_lines);
    return hash == 0u ? 1u : hash;
}

bool clearra_buildup_memo_key_equals_exact(
    const clr_buildup_memo_key *left,
    const clr_buildup_memo_key *right) {
    if (left == 0 || right == 0) {
        return false;
    }
    return left->cache_identity_hash == right->cache_identity_hash &&
           left->remaining_ops_bitset == right->remaining_ops_bitset &&
           left->current_board_mask == right->current_board_mask &&
           left->deleted_line_state.deleted_row_mask ==
               right->deleted_line_state.deleted_row_mask &&
           left->deleted_line_state.deleted_count ==
               right->deleted_line_state.deleted_count &&
           left->hold_automaton_state.piece_source_id ==
               right->hold_automaton_state.piece_source_id &&
           left->hold_automaton_state.cursor ==
               right->hold_automaton_state.cursor &&
           left->hold_automaton_state.bag_epoch ==
               right->hold_automaton_state.bag_epoch &&
           left->hold_automaton_state.bag_remainder_key ==
               right->hold_automaton_state.bag_remainder_key &&
           left->hold_automaton_state.provenance_id ==
               right->hold_automaton_state.provenance_id &&
           left->hold_automaton_state.hold_piece ==
               right->hold_automaton_state.hold_piece &&
           left->hold_automaton_state.hold_empty ==
               right->hold_automaton_state.hold_empty &&
           left->hold_automaton_state.terminal_projection_consumed ==
               right->hold_automaton_state.terminal_projection_consumed &&
           left->hold_automaton_state.terminal_projection_provenance ==
               right->hold_automaton_state.terminal_projection_provenance &&
           left->piece_source_cursor == right->piece_source_cursor &&
           left->reachability_relevant_state ==
               right->reachability_relevant_state &&
           left->cleared_lines == right->cleared_lines;
}

bool clearra_buildup_memo_key_matches_bucket(
    const clr_buildup_memo_key *left,
    uint64_t left_hash,
    const clr_buildup_memo_key *right,
    uint64_t right_hash) {
    return left_hash == right_hash &&
           clearra_buildup_memo_key_equals_exact(left, right);
}

ClearraBuildUpState clearra_buildup_state_initial(
    const clr_buildup_problem *problem) {
    ClearraBuildUpState state = {0};
    if (problem == 0) {
        return state;
    }

    state.board_mask = problem->initial_board.initial_mask;
    state.hold_automaton_state = problem->initial_hold_automaton;
    state.reachability_relevant_state = problem->initial_board.initial_mask;
    state.placed_pieces = 0;
    state.cleared_lines = 0;
    state.line_clear_state.deleted_row_mask = 0;
    state.line_clear_state.deleted_count = 0;
    state.line_clear_state.reserved = 0;
    return state;
}
