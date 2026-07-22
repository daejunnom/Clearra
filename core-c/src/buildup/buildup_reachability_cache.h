#ifndef CLEARRA_BUILDUP_REACHABILITY_CACHE_H
#define CLEARRA_BUILDUP_REACHABILITY_CACHE_H

#include "buildup_reachability_result.h"

#define CLEARRA_BUILDUP_REACHABILITY_CACHE_LINE_BYTES 64u

typedef struct ClearraBuildUpReachabilityCacheEntry {
    uint64_t board_mask;
    uint64_t operation_key;
    ClearraBuildUpReachabilityResult result;
    uint32_t status;
    uint8_t reserved[20];
} ClearraBuildUpReachabilityCacheEntry;

_Static_assert(
    sizeof(ClearraBuildUpReachabilityCacheEntry) == 64u,
    "BuildUp reachability cache entries must fit one cache line");

typedef struct ClearraBuildUpReachabilityCache {
    ClearraBuildUpReachabilityCacheEntry *entries;
    void *entries_allocation;
    uint32_t *epochs;
    uint32_t capacity;
    uint32_t epoch;
    uint64_t insertion_count;
    uint64_t collision_count;
} ClearraBuildUpReachabilityCache;

void clearra_buildup_reachability_cache_prepare(
    ClearraBuildUpReachabilityCache *cache,
    const clr_buildup_problem *problem,
    bool reset_entries);
void clearra_buildup_reachability_cache_release(
    ClearraBuildUpReachabilityCache *cache);
bool clearra_buildup_reachability_cache_lookup(
    const ClearraBuildUpReachabilityCache *cache,
    uint64_t board_mask,
    const clr_buildup_operation *operation,
    int8_t adjusted_y,
    uint8_t mode,
    uint8_t trace_mode,
    clr_buildup_status *out_status,
    ClearraBuildUpReachabilityResult *out_result);
void clearra_buildup_reachability_cache_insert(
    ClearraBuildUpReachabilityCache *cache,
    uint64_t board_mask,
    const clr_buildup_operation *operation,
    int8_t adjusted_y,
    uint8_t mode,
    uint8_t trace_mode,
    clr_buildup_status status,
    const ClearraBuildUpReachabilityResult *result);

#endif
