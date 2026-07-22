#ifndef CLEARRA_BUILDUP_REACHABLE_LOCK_CACHE_H
#define CLEARRA_BUILDUP_REACHABLE_LOCK_CACHE_H

#include "buildup_reachability_result.h"
#include "../reachability/reachable_lock_batch.h"

#define CLEARRA_BUILDUP_REACHABLE_LOCK_CACHE_LINE_BYTES 64u

typedef struct ClearraBuildUpReachableLockCacheEntry {
    uint64_t board_mask;
    uint64_t anchor_bits[CLEARRA_ROTATION_STATE_COUNT];
    uint16_t visited_state_count;
    uint8_t piece;
    uint8_t mode;
    uint8_t complete;
    uint8_t reserved[19];
} ClearraBuildUpReachableLockCacheEntry;

_Static_assert(
    sizeof(ClearraBuildUpReachableLockCacheEntry) == 64u,
    "BuildUp reachable-lock cache entries must fit one cache line");

typedef struct ClearraBuildUpReachableLockCache {
    ClearraBuildUpReachableLockCacheEntry *entries;
    void *entries_allocation;
    uint32_t *epochs;
    uint32_t capacity;
    uint32_t epoch;
    uint64_t insertion_count;
    uint64_t collision_count;
} ClearraBuildUpReachableLockCache;

void clearra_buildup_reachable_lock_cache_prepare(
    ClearraBuildUpReachableLockCache *cache,
    const clr_buildup_problem *problem,
    bool reset_entries);
void clearra_buildup_reachable_lock_cache_release(
    ClearraBuildUpReachableLockCache *cache);
clr_buildup_status clearra_buildup_reachable_lock_cache_check(
    ClearraBuildUpReachableLockCache *cache,
    ClearraBoard64Layout layout,
    uint64_t board_mask,
    const clr_buildup_operation *operation,
    int8_t adjusted_y,
    const ClearraCompactRuleProfile *compiled_rule,
    uint8_t mode,
    ClearraReachabilityFrontier *frontier,
    bool *out_cache_hit,
    ClearraBuildUpReachabilityResult *out_result);

#endif
