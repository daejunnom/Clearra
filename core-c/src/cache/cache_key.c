#include "cache_identity.h"

#define CLEARRA_CACHE_FNV_OFFSET UINT64_C(1469598103934665603)
#define CLEARRA_CACHE_FNV_PRIME UINT64_C(1099511628211)
uint64_t clearra_cache_key_mix_u64(uint64_t hash, uint64_t value) {
    hash ^= value;
    hash *= CLEARRA_CACHE_FNV_PRIME;
    return hash;
}uint64_t clearra_cache_identity_hash(ClearraCacheIdentity identity) {
    uint64_t hash = CLEARRA_CACHE_FNV_OFFSET;
    hash = clearra_cache_key_mix_u64(hash, UINT64_C(0x434c524341434830));
    hash = clearra_cache_key_mix_u64(hash, identity.board);
    hash = clearra_cache_key_mix_u64(hash, identity.piece_set_profile);
    hash = clearra_cache_key_mix_u64(hash, identity.piece_definition_id_fingerprint);
    hash = clearra_cache_key_mix_u64(hash, identity.piece_area_multiset_fingerprint);
    hash = clearra_cache_key_mix_u64(hash, identity.rule_kick_profile);
    hash = clearra_cache_key_mix_u64(hash, identity.backend_mode);
    hash = clearra_cache_key_mix_u64(hash, identity.operation_table_version);
    hash = clearra_cache_key_mix_u64(hash, identity.supply_provenance);
    hash = clearra_cache_key_mix_u64(hash, identity.queue_pattern_id);
    hash = clearra_cache_key_mix_u64(hash, identity.piece_window_start);
    hash = clearra_cache_key_mix_u64(hash, identity.piece_window_len);
    hash = clearra_cache_key_mix_u64(hash, identity.goal_id);
    return hash;
}