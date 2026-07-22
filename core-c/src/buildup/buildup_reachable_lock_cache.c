#include "buildup_reachable_lock_cache.h"

#include <limits.h>
#include <stdlib.h>
#include <string.h>

#define CLEARRA_BUILDUP_REACHABLE_LOCK_CACHE_DEFAULT_BYTES \
    (UINT64_C(2) * UINT64_C(1024) * UINT64_C(1024))
#define CLEARRA_BUILDUP_REACHABLE_LOCK_CACHE_INITIAL_BYTES \
    (UINT64_C(256) * UINT64_C(1024))
#define CLEARRA_BUILDUP_REACHABLE_LOCK_CACHE_MIN_CAPACITY 256u

static uint64_t per_worker_budget(const clr_buildup_problem *problem) {
    if (problem->packing.budget.has_max_memory_mib == 0u) {
        return CLEARRA_BUILDUP_REACHABLE_LOCK_CACHE_DEFAULT_BYTES;
    }
    uint64_t workers = problem->packing.backend.workers == 0u
                           ? UINT64_C(1)
                           : problem->packing.backend.workers;
    uint64_t memory_bytes =
        (uint64_t)problem->packing.budget.max_memory_mib *
        UINT64_C(1024) * UINT64_C(1024);
    return memory_bytes / UINT64_C(16) / workers;
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
    uint64_t bytes_per_entry =
        sizeof(ClearraBuildUpReachableLockCacheEntry) + sizeof(uint32_t);
    uint64_t entry_count = per_worker_budget(problem) / bytes_per_entry;
    uint64_t allocation_limit =
        SIZE_MAX / sizeof(ClearraBuildUpReachableLockCacheEntry);
    uint64_t epoch_limit = SIZE_MAX / sizeof(uint32_t);
    if (allocation_limit > epoch_limit) {
        allocation_limit = epoch_limit;
    }
    if (entry_count > allocation_limit) {
        entry_count = allocation_limit;
    }
    return entry_count < CLEARRA_BUILDUP_REACHABLE_LOCK_CACHE_MIN_CAPACITY
               ? 0u
               : lower_power_of_two(entry_count);
}

static uint32_t initial_capacity(uint32_t maximum) {
    const uint64_t bytes_per_entry =
        sizeof(ClearraBuildUpReachableLockCacheEntry) + sizeof(uint32_t);
    uint32_t initial = lower_power_of_two(
        CLEARRA_BUILDUP_REACHABLE_LOCK_CACHE_INITIAL_BYTES /
        bytes_per_entry);
    if (initial < CLEARRA_BUILDUP_REACHABLE_LOCK_CACHE_MIN_CAPACITY) {
        initial = CLEARRA_BUILDUP_REACHABLE_LOCK_CACHE_MIN_CAPACITY;
    }
    return initial < maximum ? initial : maximum;
}

static uint32_t selected_capacity(
    const ClearraBuildUpReachableLockCache *cache,
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

static void *aligned_allocation(size_t bytes, void **out_allocation) {
    if (out_allocation == 0 ||
        bytes > SIZE_MAX - CLEARRA_BUILDUP_REACHABLE_LOCK_CACHE_LINE_BYTES) {
        return 0;
    }
    void *allocation = malloc(
        bytes + CLEARRA_BUILDUP_REACHABLE_LOCK_CACHE_LINE_BYTES - 1u);
    if (allocation == 0) {
        return 0;
    }
    uintptr_t address = (uintptr_t)allocation;
    uintptr_t aligned =
        (address + CLEARRA_BUILDUP_REACHABLE_LOCK_CACHE_LINE_BYTES - 1u) &
        ~(uintptr_t)(CLEARRA_BUILDUP_REACHABLE_LOCK_CACHE_LINE_BYTES - 1u);
    *out_allocation = allocation;
    return (void *)aligned;
}

static void advance_epoch(ClearraBuildUpReachableLockCache *cache) {
    cache->epoch++;
    if (cache->epoch != 0u) {
        return;
    }
    memset(cache->epochs, 0, (size_t)cache->capacity * sizeof(*cache->epochs));
    cache->epoch = 1u;
}

void clearra_buildup_reachable_lock_cache_prepare(
    ClearraBuildUpReachableLockCache *cache,
    const clr_buildup_problem *problem,
    bool reset_entries) {
    if (cache == 0 || problem == 0) {
        return;
    }
    uint32_t maximum = maximum_capacity(problem);
    if (maximum == 0u) {
        clearra_buildup_reachable_lock_cache_release(cache);
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
        (size_t)capacity * sizeof(ClearraBuildUpReachableLockCacheEntry);
    void *entries_allocation = 0;
    ClearraBuildUpReachableLockCacheEntry *entries =
        (ClearraBuildUpReachableLockCacheEntry *)aligned_allocation(
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
            clearra_buildup_reachable_lock_cache_release(cache);
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

void clearra_buildup_reachable_lock_cache_release(
    ClearraBuildUpReachableLockCache *cache) {
    if (cache == 0) {
        return;
    }
    free(cache->entries_allocation);
    free(cache->epochs);
    *cache = (ClearraBuildUpReachableLockCache){0};
}

static uint32_t cache_index(
    const ClearraBuildUpReachableLockCache *cache,
    uint64_t board_mask,
    uint8_t piece,
    uint8_t mode) {
    uint64_t hash = board_mask ^
                    ((uint64_t)piece << 56u) ^
                    ((uint64_t)mode << 48u) ^
                    UINT64_C(0x9e3779b97f4a7c15);
    hash = (hash ^ (hash >> 30u)) * UINT64_C(0xbf58476d1ce4e5b9);
    hash = (hash ^ (hash >> 27u)) * UINT64_C(0x94d049bb133111eb);
    hash ^= hash >> 31u;
    return (uint32_t)hash & (cache->capacity - 1u);
}

static clr_buildup_status status_from_candidate(
    ClearraCandidateStatus status) {
    if (status == CLEARRA_CANDIDATE_OK) {
        return CLR_BUILDUP_OK;
    }
    if (status == CLEARRA_CANDIDATE_CAPACITY_EXCEEDED) {
        return CLR_BUILDUP_CAPACITY_EXCEEDED;
    }
    return CLR_BUILDUP_INVALID_PROBLEM;
}

static bool entry_contains(
    const ClearraBuildUpReachableLockCacheEntry *entry,
    ClearraBoard64Layout layout,
    const clr_buildup_operation *operation,
    int8_t adjusted_y) {
    ClearraReachableLockSet set = {
        .anchor_bits = {
            entry->anchor_bits[0],
            entry->anchor_bits[1],
            entry->anchor_bits[2],
            entry->anchor_bits[3],
        },
        .visited_state_count = entry->visited_state_count,
        .complete = entry->complete,
    };
    return clearra_reachable_lock_set_contains(
        &set,
        layout,
        operation->rotation,
        operation->x,
        adjusted_y);
}

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
    ClearraBuildUpReachabilityResult *out_result) {
    if (cache == 0 || operation == 0 || compiled_rule == 0 ||
        frontier == 0 || out_result == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    if (out_cache_hit != 0) {
        *out_cache_hit = false;
    }
    *out_result = (ClearraBuildUpReachabilityResult){0};

    uint32_t index = cache->capacity == 0u
                         ? 0u
                         : cache_index(cache, board_mask, operation->piece, mode);
    const ClearraBuildUpReachableLockCacheEntry *entry = 0;
    if (cache->entries != 0 && cache->epochs != 0 &&
        cache->capacity != 0u && cache->epochs[index] == cache->epoch) {
        const ClearraBuildUpReachableLockCacheEntry *candidate =
            &cache->entries[index];
        if (candidate->board_mask == board_mask &&
            candidate->piece == operation->piece && candidate->mode == mode &&
            candidate->complete != 0u) {
            entry = candidate;
            if (out_cache_hit != 0) {
                *out_cache_hit = true;
            }
        }
    }

    ClearraReachableLockSet generated;
    if (entry == 0) {
        ClearraCandidateStatus candidate_status =
            clearra_reachable_lock_batch_generate(
                layout,
                board_mask,
                operation->piece,
                compiled_rule,
                mode,
                frontier,
                &generated);
        clr_buildup_status status = status_from_candidate(candidate_status);
        if (status != CLR_BUILDUP_OK || generated.complete == 0u) {
            return status == CLR_BUILDUP_OK ? CLR_BUILDUP_CAPACITY_EXCEEDED
                                            : status;
        }
        if (cache->entries != 0 && cache->epochs != 0 &&
            cache->capacity != 0u) {
            ClearraBuildUpReachableLockCacheEntry *destination =
                &cache->entries[index];
            cache->insertion_count++;
            if (cache->epochs[index] == cache->epoch &&
                (destination->board_mask != board_mask ||
                 destination->piece != operation->piece ||
                 destination->mode != mode)) {
                cache->collision_count++;
            }
            destination->board_mask = board_mask;
            memcpy(
                destination->anchor_bits,
                generated.anchor_bits,
                sizeof(destination->anchor_bits));
            destination->visited_state_count = generated.visited_state_count;
            destination->piece = operation->piece;
            destination->mode = mode;
            destination->complete = 1u;
            cache->epochs[index] = cache->epoch;
            entry = destination;
        } else {
            bool reachable = clearra_reachable_lock_set_contains(
                &generated,
                layout,
                operation->rotation,
                operation->x,
                adjusted_y);
            out_result->visited_states = generated.visited_state_count;
            out_result->flags = reachable ? CLEARRA_BUILDUP_REACHABLE_FLAG : 0u;
            return reachable ? CLR_BUILDUP_OK
                             : CLR_BUILDUP_REACHABILITY_IMPOSSIBLE;
        }
    }

    bool reachable = entry_contains(entry, layout, operation, adjusted_y);
    out_result->visited_states = entry->visited_state_count;
    out_result->flags = reachable ? CLEARRA_BUILDUP_REACHABLE_FLAG : 0u;
    return reachable ? CLR_BUILDUP_OK : CLR_BUILDUP_REACHABILITY_IMPOSSIBLE;
}
