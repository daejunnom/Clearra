#include "buildup_reachability_cache.h"

#include <limits.h>
#include <stdlib.h>
#include <string.h>

#define CLEARRA_BUILDUP_REACHABILITY_CACHE_DEFAULT_PER_WORKER_BYTES \
    (UINT64_C(16) * UINT64_C(1024) * UINT64_C(1024))
#define CLEARRA_BUILDUP_REACHABILITY_CACHE_INITIAL_PER_WORKER_BYTES \
    (UINT64_C(512) * UINT64_C(1024))
#define CLEARRA_BUILDUP_REACHABILITY_CACHE_MIN_CAPACITY 256u

static uint64_t cache_per_worker_budget(const clr_buildup_problem *problem) {
    if (problem->packing.budget.has_max_memory_mib == 0u) {
        return CLEARRA_BUILDUP_REACHABILITY_CACHE_DEFAULT_PER_WORKER_BYTES;
    }
    uint64_t workers = problem->packing.backend.workers == 0u
                           ? UINT64_C(1)
                           : problem->packing.backend.workers;
    uint64_t memory_bytes =
        (uint64_t)problem->packing.budget.max_memory_mib * UINT64_C(1024) *
        UINT64_C(1024);
    return memory_bytes / UINT64_C(8) / workers;
}

static uint32_t lower_power_of_two(uint64_t value) {
    uint32_t result = 1u;
    uint64_t maximum = UINT64_C(1) << 31u;
    value = value > maximum ? maximum : value;
    while ((uint64_t)result * UINT64_C(2) <= value) {
        result *= 2u;
    }
    return result;
}

static uint32_t maximum_capacity(const clr_buildup_problem *problem) {
    uint64_t per_worker_bytes = cache_per_worker_budget(problem);
    uint64_t bytes_per_entry =
        sizeof(ClearraBuildUpReachabilityCacheEntry) + sizeof(uint32_t);
    uint64_t entry_count = per_worker_bytes / bytes_per_entry;
    uint64_t allocation_limit =
        (uint64_t)(SIZE_MAX / sizeof(ClearraBuildUpReachabilityCacheEntry));
    uint64_t epoch_limit = (uint64_t)(SIZE_MAX / sizeof(uint32_t));
    if (allocation_limit > epoch_limit) {
        allocation_limit = epoch_limit;
    }
    if (entry_count > allocation_limit) {
        entry_count = allocation_limit;
    }
    if (entry_count < CLEARRA_BUILDUP_REACHABILITY_CACHE_MIN_CAPACITY) {
        return 0u;
    }
    return lower_power_of_two(entry_count);
}

static uint32_t initial_capacity(uint32_t maximum) {
    const uint64_t bytes_per_entry =
        sizeof(ClearraBuildUpReachabilityCacheEntry) + sizeof(uint32_t);
    uint64_t entry_count =
        CLEARRA_BUILDUP_REACHABILITY_CACHE_INITIAL_PER_WORKER_BYTES /
        bytes_per_entry;
    uint32_t initial = lower_power_of_two(entry_count);
    if (initial < CLEARRA_BUILDUP_REACHABILITY_CACHE_MIN_CAPACITY) {
        initial = CLEARRA_BUILDUP_REACHABILITY_CACHE_MIN_CAPACITY;
    }
    return initial < maximum ? initial : maximum;
}

static uint32_t selected_capacity(
    const ClearraBuildUpReachabilityCache *cache,
    uint32_t maximum) {
    if (cache->capacity == 0u || cache->capacity > maximum) {
        return initial_capacity(maximum);
    }
    bool under_pressure =
        cache->insertion_count >= cache->capacity / 4u &&
        cache->collision_count >= cache->insertion_count / 4u;
    if (!under_pressure || cache->capacity >= maximum) {
        return cache->capacity;
    }
    uint32_t grown = cache->capacity > UINT32_MAX / 2u
        ? maximum
        : cache->capacity * 2u;
    return grown < maximum ? grown : maximum;
}

static void *cache_line_aligned_allocation(size_t bytes, void **out_allocation) {
    if (out_allocation == 0 ||
        bytes > SIZE_MAX - CLEARRA_BUILDUP_REACHABILITY_CACHE_LINE_BYTES) {
        return 0;
    }
    void *allocation =
        malloc(bytes + CLEARRA_BUILDUP_REACHABILITY_CACHE_LINE_BYTES - 1u);
    if (allocation == 0) {
        return 0;
    }
    uintptr_t address = (uintptr_t)allocation;
    uintptr_t aligned =
        (address + CLEARRA_BUILDUP_REACHABILITY_CACHE_LINE_BYTES - 1u) &
        ~(uintptr_t)(CLEARRA_BUILDUP_REACHABILITY_CACHE_LINE_BYTES - 1u);
    *out_allocation = allocation;
    return (void *)aligned;
}

static void advance_epoch(ClearraBuildUpReachabilityCache *cache) {
    cache->epoch++;
    if (cache->epoch != 0u) {
        return;
    }
    memset(cache->epochs, 0, (size_t)cache->capacity * sizeof(*cache->epochs));
    cache->epoch = 1u;
}

void clearra_buildup_reachability_cache_prepare(
    ClearraBuildUpReachabilityCache *cache,
    const clr_buildup_problem *problem,
    bool reset_entries) {
    if (cache == 0 || problem == 0) {
        return;
    }
    uint32_t maximum = maximum_capacity(problem);
    if (maximum == 0u) {
        clearra_buildup_reachability_cache_release(cache);
        return;
    }
    uint32_t capacity = selected_capacity(cache, maximum);
    if (cache->entries != 0 && cache->epochs != 0 &&
        cache->capacity == capacity) {
        if (reset_entries) {
            advance_epoch(cache);
        }
        cache->insertion_count = 0u;
        cache->collision_count = 0u;
        return;
    }

    size_t entry_bytes =
        (size_t)capacity * sizeof(ClearraBuildUpReachabilityCacheEntry);
    void *entries_allocation = 0;
    ClearraBuildUpReachabilityCacheEntry *entries =
        (ClearraBuildUpReachabilityCacheEntry *)cache_line_aligned_allocation(
            entry_bytes, &entries_allocation);
    uint32_t *epochs = (uint32_t *)malloc((size_t)capacity * sizeof(*epochs));
    if (entries == 0 || epochs == 0) {
        free(entries_allocation);
        free(epochs);
        if (cache->entries != 0 && cache->epochs != 0 &&
            cache->capacity <= capacity) {
            if (reset_entries) {
                advance_epoch(cache);
            }
            cache->insertion_count = 0u;
            cache->collision_count = 0u;
        } else {
            clearra_buildup_reachability_cache_release(cache);
        }
        return;
    }
    memset(epochs, 0, (size_t)capacity * sizeof(*epochs));
    free(cache->entries_allocation);
    free(cache->epochs);
    cache->entries = entries;
    cache->entries_allocation = entries_allocation;
    cache->epochs = epochs;
    cache->capacity = capacity;
    cache->epoch = 1u;
    cache->insertion_count = 0u;
    cache->collision_count = 0u;
}

void clearra_buildup_reachability_cache_release(
    ClearraBuildUpReachabilityCache *cache) {
    if (cache == 0) {
        return;
    }
    free(cache->entries_allocation);
    free(cache->epochs);
    *cache = (ClearraBuildUpReachabilityCache){0};
}

static uint64_t operation_key(
    const clr_buildup_operation *operation,
    int8_t adjusted_y,
    uint8_t mode,
    uint8_t trace_mode) {
    return (uint64_t)operation->piece |
           ((uint64_t)operation->rotation << 8u) |
           ((uint64_t)(uint8_t)operation->x << 16u) |
           ((uint64_t)(uint8_t)adjusted_y << 24u) |
           ((uint64_t)mode << 32u) |
           ((uint64_t)trace_mode << 40u);
}

static uint32_t cache_index(
    const ClearraBuildUpReachabilityCache *cache,
    uint64_t board_mask,
    uint64_t key) {
    uint64_t hash = board_mask ^ (key + UINT64_C(0x9e3779b97f4a7c15));
    hash = (hash ^ (hash >> 30u)) * UINT64_C(0xbf58476d1ce4e5b9);
    hash = (hash ^ (hash >> 27u)) * UINT64_C(0x94d049bb133111eb);
    hash ^= hash >> 31u;
    return (uint32_t)hash & (cache->capacity - 1u);
}

bool clearra_buildup_reachability_cache_lookup(
    const ClearraBuildUpReachabilityCache *cache,
    uint64_t board_mask,
    const clr_buildup_operation *operation,
    int8_t adjusted_y,
    uint8_t mode,
    uint8_t trace_mode,
    clr_buildup_status *out_status,
    ClearraBuildUpReachabilityResult *out_result) {
    if (cache == 0 || cache->entries == 0 || cache->epochs == 0 ||
        cache->capacity == 0u || operation == 0 || out_status == 0 ||
        out_result == 0) {
        return false;
    }
    uint64_t key = operation_key(operation, adjusted_y, mode, trace_mode);
    uint32_t index = cache_index(cache, board_mask, key);
    if (cache->epochs[index] != cache->epoch) {
        return false;
    }
    const ClearraBuildUpReachabilityCacheEntry *entry = &cache->entries[index];
    if (entry->board_mask != board_mask || entry->operation_key != key) {
        return false;
    }
    *out_status = (clr_buildup_status)entry->status;
    *out_result = entry->result;
    return true;
}

void clearra_buildup_reachability_cache_insert(
    ClearraBuildUpReachabilityCache *cache,
    uint64_t board_mask,
    const clr_buildup_operation *operation,
    int8_t adjusted_y,
    uint8_t mode,
    uint8_t trace_mode,
    clr_buildup_status status,
    const ClearraBuildUpReachabilityResult *result) {
    if (cache == 0 || cache->entries == 0 || cache->epochs == 0 ||
        cache->capacity == 0u || operation == 0 || result == 0 ||
        (status != CLR_BUILDUP_OK &&
         status != CLR_BUILDUP_REACHABILITY_IMPOSSIBLE)) {
        return;
    }
    uint64_t key = operation_key(operation, adjusted_y, mode, trace_mode);
    uint32_t index = cache_index(cache, board_mask, key);
    ClearraBuildUpReachabilityCacheEntry *entry = &cache->entries[index];
    cache->insertion_count++;
    if (cache->epochs[index] == cache->epoch &&
        (entry->board_mask != board_mask || entry->operation_key != key)) {
        cache->collision_count++;
    }
    entry->board_mask = board_mask;
    entry->operation_key = key;
    entry->result = *result;
    entry->status = (uint32_t)status;
    cache->epochs[index] = cache->epoch;
}
