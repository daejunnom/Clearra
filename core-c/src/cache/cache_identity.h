#ifndef CLEARRA_CACHE_IDENTITY_H
#define CLEARRA_CACHE_IDENTITY_H

#include <stdbool.h>
#include <stdint.h>

#include "clr_problem.h"

typedef struct ClearraCacheIdentity {
    uint64_t board;
    uint64_t piece_set_profile;
    uint64_t piece_definition_id_fingerprint;
    uint64_t piece_area_multiset_fingerprint;
    uint64_t rule_kick_profile;
    uint32_t backend_mode;
    uint32_t operation_table_version;
    uint64_t supply_provenance;
    uint32_t queue_pattern_id;
    uint16_t piece_window_start;
    uint16_t piece_window_len;
    uint64_t goal_id;
} ClearraCacheIdentity;
ClearraCacheIdentity clearra_cache_identity_zero(void);
bool clearra_cache_identity_is_complete(ClearraCacheIdentity identity);
ClearraCacheIdentity clearra_cache_identity_from_packing_problem(
    const clr_packing_problem *problem,
    uint32_t operation_table_version);
uint64_t clearra_cache_key_mix_u64(uint64_t hash, uint64_t value);
uint64_t clearra_cache_identity_hash(ClearraCacheIdentity identity);
#endif
