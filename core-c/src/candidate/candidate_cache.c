#include "candidate.h"
uint64_t clearra_candidate_cache_key(
    ClearraCacheIdentity identity,
    uint8_t active_piece,
    uint8_t rule_kick_mode) {
    uint64_t hash = clearra_cache_identity_hash(identity);
    hash = clearra_cache_key_mix_u64(hash, UINT64_C(0x43414e4449440000));
    hash = clearra_cache_key_mix_u64(hash, active_piece);
    hash = clearra_cache_key_mix_u64(hash, rule_kick_mode);
    return hash;
}void clearra_candidate_cache_entry_clear(ClearraCandidateCacheEntry *entry) {
    if (entry != 0) {
        entry->key = 0;
        entry->count = 0;
        entry->occupied = false;
    }
}void clearra_candidate_cache_entry_store(
    ClearraCandidateCacheEntry *entry,
    uint64_t key,
    uint16_t count) {
    if (entry != 0) {
        entry->key = key;
        entry->count = count;
        entry->occupied = true;
    }
}bool clearra_candidate_cache_entry_matches(
    ClearraCandidateCacheEntry entry,
    uint64_t key) {
    return entry.occupied && entry.key == key;
}