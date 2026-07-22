#include "clr_hold_automaton.h"
static void clearra_hash_u8(uint64_t *hash, uint8_t value) {
    *hash ^= (uint64_t)value;
    *hash *= 1099511628211ull;
}static void clearra_hash_u16(uint64_t *hash, uint16_t value) {
    clearra_hash_u8(hash, (uint8_t)(value & 0xffu));
    clearra_hash_u8(hash, (uint8_t)((value >> 8u) & 0xffu));
}static void clearra_hash_u64(uint64_t *hash, uint64_t value) {
    for (uint32_t index = 0u; index < 8u; ++index) {
        clearra_hash_u8(hash, (uint8_t)((value >> (index * 8u)) & 0xffu));
    }
}clr_buildup_hold_automaton_memo_key clearra_buildup_hold_automaton_memo_key(
    const clr_hold_automaton_state *state) {
    clr_buildup_hold_automaton_memo_key key = {
        0u, 0u, 0u, 0u, 0u, CLR_PIECE_NONE, 1u, {0u, 0u, 0u, 0u, 0u, 0u}};
    if (state == 0) {
        return key;
    }
    key.piece_source_id = state->piece_source_id;
    key.cursor = state->cursor;
    key.bag_epoch = state->bag_epoch;
    key.bag_remainder_key = state->bag_remainder_key;
    key.provenance_id = state->provenance_id;
    key.hold_piece = state->hold_piece;
    key.hold_empty = state->hold_empty;
    return key;
}uint64_t clearra_buildup_hold_automaton_memo_key_hash(
    const clr_buildup_hold_automaton_memo_key *key) {
    if (key == 0) {
        return 0u;
    }
    uint64_t hash = 14695981039346656037ull;
    clearra_hash_u64(&hash, key->piece_source_id);
    clearra_hash_u16(&hash, key->cursor);
    clearra_hash_u16(&hash, key->bag_epoch);
    clearra_hash_u64(&hash, key->bag_remainder_key);
    clearra_hash_u64(&hash, key->provenance_id);
    clearra_hash_u8(&hash, key->hold_piece);
    clearra_hash_u8(&hash, key->hold_empty);
    return hash == 0u ? 1u : hash;
}