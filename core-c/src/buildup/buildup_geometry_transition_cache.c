#include "buildup_geometry_transition_cache.h"

#include <stdlib.h>
#include <string.h>

#define CLEARRA_BUILDUP_GEOMETRY_TRANSITION_CACHE_DEFAULT_PER_WORKER_BYTES \
    (UINT64_C(16) * UINT64_C(1024) * UINT64_C(1024))
#define CLEARRA_BUILDUP_GEOMETRY_TRANSITION_CACHE_INITIAL_PER_WORKER_BYTES \
    (UINT64_C(1024) * UINT64_C(1024))
#define CLEARRA_BUILDUP_GEOMETRY_TRANSITION_CACHE_MIN_CAPACITY 256u

_Static_assert(
    sizeof(ClearraBuildUpGeometryTransitionKey) == 40u,
    "geometry transition cache keys must remain compact");
_Static_assert(
    sizeof(ClearraBuildUpGeometryTransitionHotResult) == 24u,
    "geometry transition cache hot results must remain compact");
_Static_assert(
    sizeof(ClearraBuildUpGeometryTransitionHotEntry) ==
        CLEARRA_BUILDUP_GEOMETRY_TRANSITION_CACHE_LINE_BYTES,
    "geometry transition hot key and result must fit one cache line");

static uint64_t cache_per_worker_budget(const clr_buildup_problem *problem) {
    if (problem->packing.budget.has_max_memory_mib == 0u) {
        return
            CLEARRA_BUILDUP_GEOMETRY_TRANSITION_CACHE_DEFAULT_PER_WORKER_BYTES;
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
        sizeof(ClearraBuildUpGeometryTransitionHotEntry) +
        sizeof(ClearraBuildUpGeometryTransitionColdResult) +
        sizeof(uint32_t) * UINT64_C(2);
    uint64_t entry_count = per_worker_bytes / bytes_per_entry;
    uint64_t entry_limit = (uint64_t)(
        SIZE_MAX / sizeof(ClearraBuildUpGeometryTransitionHotEntry));
    uint64_t cold_limit = (uint64_t)(
        SIZE_MAX / sizeof(ClearraBuildUpGeometryTransitionColdResult));
    uint64_t epoch_limit = (uint64_t)(SIZE_MAX / sizeof(uint32_t));
    if (entry_limit > cold_limit) {
        entry_limit = cold_limit;
    }
    if (entry_limit > epoch_limit) {
        entry_limit = epoch_limit;
    }
    if (entry_count > entry_limit) {
        entry_count = entry_limit;
    }
    if (entry_count < CLEARRA_BUILDUP_GEOMETRY_TRANSITION_CACHE_MIN_CAPACITY) {
        return 0u;
    }
    return lower_power_of_two(entry_count);
}

static uint32_t initial_capacity(uint32_t maximum) {
    const uint64_t bytes_per_entry =
        sizeof(ClearraBuildUpGeometryTransitionHotEntry) +
        sizeof(ClearraBuildUpGeometryTransitionColdResult) +
        sizeof(uint32_t) * UINT64_C(2);
    uint64_t entry_count =
        CLEARRA_BUILDUP_GEOMETRY_TRANSITION_CACHE_INITIAL_PER_WORKER_BYTES /
        bytes_per_entry;
    uint32_t initial = lower_power_of_two(entry_count);
    if (initial < CLEARRA_BUILDUP_GEOMETRY_TRANSITION_CACHE_MIN_CAPACITY) {
        initial = CLEARRA_BUILDUP_GEOMETRY_TRANSITION_CACHE_MIN_CAPACITY;
    }
    return initial < maximum ? initial : maximum;
}

static uint32_t selected_capacity(
    const ClearraBuildUpGeometryTransitionCache *cache,
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
        bytes > SIZE_MAX - CLEARRA_BUILDUP_GEOMETRY_TRANSITION_CACHE_LINE_BYTES) {
        return 0;
    }
    void *allocation = malloc(
        bytes + CLEARRA_BUILDUP_GEOMETRY_TRANSITION_CACHE_LINE_BYTES - 1u);
    if (allocation == 0) {
        return 0;
    }
    uintptr_t address = (uintptr_t)allocation;
    uintptr_t aligned =
        (address + CLEARRA_BUILDUP_GEOMETRY_TRANSITION_CACHE_LINE_BYTES - 1u) &
        ~(uintptr_t)(CLEARRA_BUILDUP_GEOMETRY_TRANSITION_CACHE_LINE_BYTES - 1u);
    *out_allocation = allocation;
    return (void *)aligned;
}

static void advance_epoch(ClearraBuildUpGeometryTransitionCache *cache) {
    cache->epoch++;
    if (cache->epoch != 0u) {
        return;
    }
    memset(cache->epochs, 0, (size_t)cache->capacity * sizeof(*cache->epochs));
    cache->epoch = 1u;
}

void clearra_buildup_geometry_transition_cache_prepare(
    ClearraBuildUpGeometryTransitionCache *cache,
    const clr_buildup_problem *problem,
    bool reset_entries) {
    if (cache == 0 || problem == 0) {
        return;
    }
    uint32_t maximum = maximum_capacity(problem);
    if (maximum == 0u) {
        clearra_buildup_geometry_transition_cache_release(cache);
        return;
    }
    uint32_t capacity = selected_capacity(cache, maximum);
    if (cache->hot_entries != 0 && cache->epochs != 0 &&
        cache->capacity == capacity) {
        if (reset_entries) {
            advance_epoch(cache);
        }
        cache->insertion_count = 0u;
        cache->collision_count = 0u;
        return;
    }

    size_t hot_entry_bytes =
        (size_t)capacity * sizeof(ClearraBuildUpGeometryTransitionHotEntry);
    void *hot_entries_allocation = 0;
    ClearraBuildUpGeometryTransitionHotEntry *hot_entries =
        (ClearraBuildUpGeometryTransitionHotEntry *)
            cache_line_aligned_allocation(
                hot_entry_bytes, &hot_entries_allocation);
    uint32_t *epochs = (uint32_t *)malloc((size_t)capacity * sizeof(*epochs));
    if (hot_entries == 0 || epochs == 0) {
        free(hot_entries_allocation);
        free(epochs);
        if (cache->hot_entries != 0 && cache->epochs != 0 &&
            cache->capacity <= capacity) {
            if (reset_entries) {
                advance_epoch(cache);
            }
            cache->insertion_count = 0u;
            cache->collision_count = 0u;
        } else {
            clearra_buildup_geometry_transition_cache_release(cache);
        }
        return;
    }
    memset(epochs, 0, (size_t)capacity * sizeof(*epochs));
    free(cache->hot_entries_allocation);
    free(cache->cold_results_allocation);
    free(cache->epochs);
    free(cache->cold_epochs);
    cache->hot_entries = hot_entries;
    cache->hot_entries_allocation = hot_entries_allocation;
    cache->cold_results = 0;
    cache->cold_results_allocation = 0;
    cache->epochs = epochs;
    cache->cold_epochs = 0;
    cache->capacity = capacity;
    cache->epoch = 1u;
    cache->insertion_count = 0u;
    cache->collision_count = 0u;
}

void clearra_buildup_geometry_transition_cache_release(
    ClearraBuildUpGeometryTransitionCache *cache) {
    if (cache == 0) {
        return;
    }
    free(cache->hot_entries_allocation);
    free(cache->cold_results_allocation);
    free(cache->epochs);
    free(cache->cold_epochs);
    *cache = (ClearraBuildUpGeometryTransitionCache){0};
}

static bool ensure_cold_sidecar(
    ClearraBuildUpGeometryTransitionCache *cache) {
    if (cache->cold_results != 0 && cache->cold_epochs != 0) {
        return true;
    }
    if (cache->capacity == 0u) {
        return false;
    }
    size_t result_bytes =
        (size_t)cache->capacity *
        sizeof(ClearraBuildUpGeometryTransitionColdResult);
    void *results_allocation = 0;
    ClearraBuildUpGeometryTransitionColdResult *results =
        (ClearraBuildUpGeometryTransitionColdResult *)
            cache_line_aligned_allocation(result_bytes, &results_allocation);
    uint32_t *epochs =
        (uint32_t *)malloc((size_t)cache->capacity * sizeof(*epochs));
    if (results == 0 || epochs == 0) {
        free(results_allocation);
        free(epochs);
        return false;
    }
    memset(epochs, 0, (size_t)cache->capacity * sizeof(*epochs));
    cache->cold_results = results;
    cache->cold_results_allocation = results_allocation;
    cache->cold_epochs = epochs;
    return true;
}

static ClearraBuildUpGeometryTransitionKey transition_key(
    const ClearraBuildUpState *state,
    const clr_buildup_operation *operation,
    uint8_t trace_mode,
    uint8_t transition_mode) {
    return (ClearraBuildUpGeometryTransitionKey){
        .board_mask = state->board_mask,
        .operation_mask = operation->mask,
        .reachability_relevant_state = state->reachability_relevant_state,
        .deleted_row_mask = state->line_clear_state.deleted_row_mask,
        .required_deleted_row_mask = operation->required_deleted_row_mask,
        .operation_id = operation->operation_id,
        .deleted_count = state->line_clear_state.deleted_count,
        .cleared_lines = state->cleared_lines,
        .piece = operation->piece,
        .rotation = operation->rotation,
        .x = operation->x,
        .y = operation->y,
        .trace_mode = trace_mode,
        .transition_mode = transition_mode,
    };
}

static uint64_t key_hash(const ClearraBuildUpGeometryTransitionKey *key) {
    uint64_t hash = key->board_mask ^ UINT64_C(0x9e3779b97f4a7c15);
    hash ^= key->operation_mask + UINT64_C(0x517cc1b727220a95);
    hash ^= key->reachability_relevant_state + UINT64_C(0x94d049bb133111eb);
    hash ^= (uint64_t)key->deleted_row_mask << 48u;
    hash ^= (uint64_t)key->required_deleted_row_mask << 32u;
    hash ^= (uint64_t)key->operation_id << 16u;
    hash ^= (uint64_t)key->deleted_count << 24u;
    hash ^= (uint64_t)key->cleared_lines << 16u;
    hash ^= (uint64_t)key->piece << 8u;
    hash ^= (uint64_t)key->rotation;
    hash ^= (uint64_t)(uint8_t)key->x << 56u;
    hash ^= (uint64_t)(uint8_t)key->y << 40u;
    hash ^= (uint64_t)key->trace_mode << 32u;
    hash ^= (uint64_t)key->transition_mode << 24u;
    hash = (hash ^ (hash >> 30u)) * UINT64_C(0xbf58476d1ce4e5b9);
    hash = (hash ^ (hash >> 27u)) * UINT64_C(0x94d049bb133111eb);
    return hash ^ (hash >> 31u);
}

static bool key_matches(
    const ClearraBuildUpGeometryTransitionKey *left,
    const ClearraBuildUpGeometryTransitionKey *right) {
    return left->board_mask == right->board_mask &&
           left->operation_mask == right->operation_mask &&
           left->reachability_relevant_state ==
               right->reachability_relevant_state &&
           left->deleted_row_mask == right->deleted_row_mask &&
           left->required_deleted_row_mask == right->required_deleted_row_mask &&
           left->operation_id == right->operation_id &&
           left->deleted_count == right->deleted_count &&
           left->cleared_lines == right->cleared_lines &&
           left->piece == right->piece && left->rotation == right->rotation &&
           left->x == right->x && left->y == right->y &&
           left->trace_mode == right->trace_mode &&
           left->transition_mode == right->transition_mode;
}

static uint32_t cache_index(
    const ClearraBuildUpGeometryTransitionCache *cache,
    const ClearraBuildUpGeometryTransitionKey *key) {
    return (uint32_t)key_hash(key) & (cache->capacity - 1u);
}

bool clearra_buildup_geometry_transition_cache_lookup(
    const ClearraBuildUpGeometryTransitionCache *cache,
    const ClearraBuildUpState *state,
    const clr_buildup_operation *operation,
    uint8_t trace_mode,
    uint8_t transition_mode,
    ClearraBuildUpGeometryTransitionResult *out_result) {
    if (cache == 0 || cache->hot_entries == 0 || cache->epochs == 0 ||
        cache->capacity == 0u || state == 0 || operation == 0 ||
        out_result == 0) {
        return false;
    }
    ClearraBuildUpGeometryTransitionKey key =
        transition_key(state, operation, trace_mode, transition_mode);
    uint32_t index = cache_index(cache, &key);
    const ClearraBuildUpGeometryTransitionHotEntry *entry =
        &cache->hot_entries[index];
    if (cache->epochs[index] != cache->epoch ||
        !key_matches(&entry->key, &key)) {
        return false;
    }
    const ClearraBuildUpGeometryTransitionHotResult *hot =
        &entry->result;
    bool needs_cold_result =
        trace_mode != CLEARRA_REACHABILITY_TRACE_NONE &&
        hot->status == CLR_BUILDUP_OK;
    if (needs_cold_result &&
        (cache->cold_results == 0 || cache->cold_epochs == 0 ||
         cache->cold_epochs[index] != cache->epoch)) {
        return false;
    }
    *out_result = (ClearraBuildUpGeometryTransitionResult){
        .board_mask = hot->board_mask,
        .reachability_relevant_state = hot->reachability_relevant_state,
        .line_clear_state = hot->line_clear_state,
        .status = hot->status,
        .cleared_lines = hot->cleared_lines,
    };
    if (needs_cold_result) {
        out_result->trace_step = cache->cold_results[index].trace_step;
        out_result->kick_evidence = cache->cold_results[index].kick_evidence;
    }
    return true;
}

void clearra_buildup_geometry_transition_cache_insert(
    ClearraBuildUpGeometryTransitionCache *cache,
    const ClearraBuildUpState *state,
    const clr_buildup_operation *operation,
    clr_buildup_status status,
    const ClearraBuildUpState *next_state,
    const clr_buildup_trace_step *trace_step,
    const clr_kick_evidence_view *kick_evidence,
    uint8_t trace_mode,
    uint8_t transition_mode) {
    if (cache == 0 || cache->hot_entries == 0 || cache->epochs == 0 ||
        cache->capacity == 0u || state == 0 || operation == 0 ||
        (status == CLR_BUILDUP_OK &&
         (next_state == 0 ||
          (trace_mode != CLEARRA_REACHABILITY_TRACE_NONE &&
           (trace_step == 0 || kick_evidence == 0)))) ||
        (status != CLR_BUILDUP_OK &&
         clearra_buildup_branch_outcome_for_status(status) !=
             CLEARRA_BUILDUP_BRANCH_LOGICAL_REJECT)) {
        return;
    }
    ClearraBuildUpGeometryTransitionKey key =
        transition_key(state, operation, trace_mode, transition_mode);
    uint32_t index = cache_index(cache, &key);
    cache->insertion_count++;
    if (cache->epochs[index] == cache->epoch &&
        !key_matches(&cache->hot_entries[index].key, &key)) {
        cache->collision_count++;
    }
    cache->hot_entries[index] = (ClearraBuildUpGeometryTransitionHotEntry){
        .key = key,
        .result = {
            .board_mask = next_state == 0 ? 0u : next_state->board_mask,
            .reachability_relevant_state =
                next_state == 0 ? 0u : next_state->reachability_relevant_state,
            .line_clear_state =
                next_state == 0 ? (ClearraLineClearState){0}
                                : next_state->line_clear_state,
            .status = (uint8_t)status,
            .cleared_lines = next_state == 0 ? 0u : next_state->cleared_lines,
        },
    };
    bool stores_cold_result =
        trace_mode != CLEARRA_REACHABILITY_TRACE_NONE &&
        status == CLR_BUILDUP_OK;
    if (stores_cold_result && ensure_cold_sidecar(cache)) {
        cache->cold_results[index] =
            (ClearraBuildUpGeometryTransitionColdResult){
                .trace_step = *trace_step,
                .kick_evidence = *kick_evidence,
            };
        cache->cold_epochs[index] = cache->epoch;
    } else if (cache->cold_epochs != 0) {
        cache->cold_epochs[index] = 0u;
    }
    cache->epochs[index] = cache->epoch;
}
