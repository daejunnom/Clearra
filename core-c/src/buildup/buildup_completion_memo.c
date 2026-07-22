#include "buildup_completion_memo.h"

#include <stdlib.h>
#include <string.h>

static uint32_t next_power_of_two(uint64_t value) {
    uint32_t result = CLEARRA_BUILDUP_COMPLETION_MEMO_MIN_CAPACITY;
    while ((uint64_t)result < value &&
           result < CLEARRA_BUILDUP_COMPLETION_MEMO_MAX_CAPACITY) {
        result <<= 1u;
    }
    return result;
}

static uint32_t memory_bounded_capacity(const clr_buildup_problem *problem) {
    uint64_t state_budget = problem->packing.budget.max_nodes;
    if (state_budget == 0u ||
        state_budget > CLEARRA_BUILDUP_COMPLETION_MEMO_MAX_CAPACITY) {
        state_budget = CLEARRA_BUILDUP_COMPLETION_MEMO_MAX_CAPACITY;
    }

    /* Keep the table below 75% load for terminating, cache-local misses. */
    uint64_t desired = state_budget + (state_budget / 3u) + 1u;
    uint32_t capacity = next_power_of_two(desired);
    if (problem->packing.budget.has_max_memory_mib != 0u) {
        uint64_t memory_bytes =
            (uint64_t)problem->packing.budget.max_memory_mib * UINT64_C(1024) *
            UINT64_C(1024);
        uint64_t workers = problem->packing.backend.workers == 0u
                               ? UINT64_C(1)
                               : problem->packing.backend.workers;
        uint64_t memo_budget = memory_bytes / UINT64_C(8) / workers;
        const uint64_t bytes_per_bucket =
            sizeof(ClearraBuildUpCompletionMemoEntry) + sizeof(uint16_t);
        if ((uint64_t)CLEARRA_BUILDUP_COMPLETION_MEMO_MIN_CAPACITY *
                bytes_per_bucket >
            memo_budget) {
            return 0u;
        }
        while (capacity > CLEARRA_BUILDUP_COMPLETION_MEMO_MIN_CAPACITY &&
               (uint64_t)capacity * bytes_per_bucket > memo_budget) {
            capacity >>= 1u;
        }
    }
    return capacity;
}

static uint32_t initial_capacity(const clr_buildup_problem *problem) {
    uint16_t operation_count = problem->operation_set.operation_count;
    uint8_t exponent = operation_count >= 13u
        ? 16u
        : (uint8_t)(operation_count + UINT16_C(3));
    uint64_t expected_states = UINT64_C(1) << exponent;
    return next_power_of_two(expected_states);
}

static uint32_t selected_capacity(
    const clr_buildup_problem *problem,
    const ClearraBuildUpCompletionMemoStorage *storage,
    uint32_t maximum) {
    if (storage == 0 || storage->entries == 0 ||
        storage->occupied_generations == 0 || storage->capacity == 0u) {
        uint32_t initial = initial_capacity(problem);
        return initial < maximum ? initial : maximum;
    }
    if (storage->capacity > maximum) {
        return maximum;
    }
    bool under_pressure = storage->saturation_skips != 0u ||
                          storage->max_probe_length > 32u;
    if (!under_pressure || storage->capacity >= maximum) {
        return storage->capacity;
    }
    uint32_t grown = storage->capacity > UINT32_MAX / 2u
        ? maximum
        : storage->capacity * 2u;
    return grown < maximum ? grown : maximum;
}

static bool memo_identity_matches(
    const ClearraBuildUpCompletionMemo *memo,
    const clr_buildup_memo_key *key) {
    return memo->cache_identity_hash == key->cache_identity_hash &&
           memo->piece_source_id == key->hold_automaton_state.piece_source_id &&
           memo->provenance_id == key->hold_automaton_state.provenance_id;
}

static bool entry_matches(
    const ClearraBuildUpCompletionMemoEntry *entry,
    const clr_buildup_memo_key *key,
    uint64_t key_hash) {
    return entry->key_hash == key_hash &&
           entry->current_board_mask == key->current_board_mask &&
           entry->reachability_relevant_state ==
               key->reachability_relevant_state &&
           entry->bag_remainder_key ==
               key->hold_automaton_state.bag_remainder_key &&
           entry->remaining_ops_bitset == key->remaining_ops_bitset &&
           entry->deleted_row_mask == key->deleted_line_state.deleted_row_mask &&
           entry->hold_cursor == key->hold_automaton_state.cursor &&
           entry->piece_source_cursor == key->piece_source_cursor &&
           entry->bag_epoch == key->hold_automaton_state.bag_epoch &&
           entry->deleted_count == key->deleted_line_state.deleted_count &&
           entry->hold_piece == key->hold_automaton_state.hold_piece &&
           entry->hold_empty == key->hold_automaton_state.hold_empty &&
           entry->cleared_lines == key->cleared_lines;
}

static void entry_store(
    ClearraBuildUpCompletionMemoEntry *entry,
    const clr_buildup_memo_key *key,
    uint64_t key_hash,
    uint64_t completion_count) {
    entry->key_hash = key_hash;
    entry->completion_count = completion_count;
    entry->current_board_mask = key->current_board_mask;
    entry->reachability_relevant_state = key->reachability_relevant_state;
    entry->bag_remainder_key = key->hold_automaton_state.bag_remainder_key;
    entry->remaining_ops_bitset = key->remaining_ops_bitset;
    entry->deleted_row_mask = key->deleted_line_state.deleted_row_mask;
    entry->hold_cursor = key->hold_automaton_state.cursor;
    entry->piece_source_cursor = key->piece_source_cursor;
    entry->bag_epoch = key->hold_automaton_state.bag_epoch;
    entry->deleted_count = key->deleted_line_state.deleted_count;
    entry->hold_piece = key->hold_automaton_state.hold_piece;
    entry->hold_empty = key->hold_automaton_state.hold_empty;
    entry->cleared_lines = key->cleared_lines;
}

static void memo_set_identity(
    ClearraBuildUpCompletionMemo *memo,
    const clr_buildup_problem *problem,
    uint32_t capacity) {
    ClearraCacheIdentity identity =
        clearra_cache_identity_from_packing_problem(&problem->packing, 1u);
    memo->capacity = capacity;
    memo->max_load = (capacity * 3u) / 4u;
    memo->cache_identity_hash = clearra_cache_identity_hash(identity);
    memo->piece_source_id = problem->initial_hold_automaton.piece_source_id;
    memo->provenance_id = problem->initial_hold_automaton.provenance_id;
}

static bool allocate_storage(
    uint32_t capacity,
    ClearraBuildUpCompletionMemoEntry **out_entries,
    uint16_t **out_occupied_generations) {
    ClearraBuildUpCompletionMemoEntry *entries =
        (ClearraBuildUpCompletionMemoEntry *)malloc(
            (size_t)capacity * sizeof(*entries));
    if (entries == 0) {
        return false;
    }
    size_t generation_bytes =
        (size_t)capacity * sizeof(*(*out_occupied_generations));
    uint16_t *occupied_generations =
        (uint16_t *)malloc(generation_bytes);
    if (occupied_generations == 0) {
        free(entries);
        return false;
    }
    memset(occupied_generations, 0, generation_bytes);
    *out_entries = entries;
    *out_occupied_generations = occupied_generations;
    return true;
}

static uint16_t next_storage_generation(
    ClearraBuildUpCompletionMemoStorage *storage) {
    uint16_t generation = (uint16_t)(storage->generation + 1u);
    if (generation == 0u) {
        memset(
            storage->occupied_generations,
            0,
            (size_t)storage->capacity *
                sizeof(*storage->occupied_generations));
        generation = 1u;
    }
    storage->generation = generation;
    return generation;
}

void clearra_buildup_completion_memo_init(
    ClearraBuildUpCompletionMemo *memo,
    const clr_buildup_problem *problem) {
    if (memo == 0) {
        return;
    }
    *memo = (ClearraBuildUpCompletionMemo){0};
    if (problem == 0) {
        return;
    }

    uint32_t maximum = memory_bounded_capacity(problem);
    if (maximum == 0u) {
        return;
    }
    uint32_t capacity = selected_capacity(problem, 0, maximum);
    if (!allocate_storage(
            capacity, &memo->entries, &memo->occupied_generations)) {
        return;
    }
    memo->generation = 1u;
    memo->owns_entries = 1u;
    memo_set_identity(memo, problem, capacity);
}

void clearra_buildup_completion_memo_init_with_storage(
    ClearraBuildUpCompletionMemo *memo,
    const clr_buildup_problem *problem,
    ClearraBuildUpCompletionMemoStorage *storage) {
    if (memo == 0) {
        return;
    }
    *memo = (ClearraBuildUpCompletionMemo){0};
    if (problem == 0 || storage == 0) {
        return;
    }

    uint32_t maximum = memory_bounded_capacity(problem);
    if (maximum == 0u) {
        clearra_buildup_completion_memo_storage_release(storage);
        return;
    }
    uint32_t capacity = selected_capacity(problem, storage, maximum);
    if (storage->entries == 0 || storage->occupied_generations == 0 ||
        storage->capacity != capacity) {
        ClearraBuildUpCompletionMemoEntry *new_entries = 0;
        uint16_t *new_occupied_generations = 0;
        if (!allocate_storage(
                capacity, &new_entries, &new_occupied_generations)) {
            if (storage->entries != 0 &&
                storage->occupied_generations != 0 &&
                storage->capacity <= maximum) {
                capacity = storage->capacity;
                storage->max_probe_length = 0u;
                storage->saturation_skips = 0u;
            } else {
                clearra_buildup_completion_memo_storage_release(storage);
            }
        } else {
            free(storage->entries);
            free(storage->occupied_generations);
            storage->entries = new_entries;
            storage->occupied_generations = new_occupied_generations;
            storage->capacity = capacity;
            storage->generation = 0u;
            storage->max_probe_length = 0u;
            storage->saturation_skips = 0u;
        }
        if (storage->entries == 0 || storage->occupied_generations == 0) {
            return;
        }
    }

    memo->entries = storage->entries;
    memo->occupied_generations = storage->occupied_generations;
    memo->storage = storage;
    memo->generation = next_storage_generation(storage);
    memo_set_identity(memo, problem, capacity);
}

void clearra_buildup_completion_memo_release(
    ClearraBuildUpCompletionMemo *memo) {
    if (memo == 0) {
        return;
    }
    if (memo->owns_entries != 0u) {
        free(memo->entries);
        free(memo->occupied_generations);
    } else if (memo->storage != 0) {
        memo->storage->max_probe_length = memo->max_probe_length;
        memo->storage->saturation_skips = memo->saturation_skips;
    }
    memo->entries = 0;
    memo->occupied_generations = 0;
    memo->storage = 0;
}

void clearra_buildup_completion_memo_storage_release(
    ClearraBuildUpCompletionMemoStorage *storage) {
    if (storage == 0) {
        return;
    }
    free(storage->entries);
    free(storage->occupied_generations);
    *storage = (ClearraBuildUpCompletionMemoStorage){0};
}

bool clearra_buildup_completion_memo_lookup(
    ClearraBuildUpCompletionMemo *memo,
    const clr_buildup_memo_key *key,
    uint64_t *out_completion_count) {
    if (memo == 0 || key == 0 || out_completion_count == 0 ||
        memo->entries == 0 || memo->occupied_generations == 0 ||
        memo->capacity == 0u ||
        !memo_identity_matches(memo, key)) {
        return false;
    }

    uint64_t key_hash = clearra_buildup_memo_key_hash(key);
    uint32_t mask = memo->capacity - 1u;
    for (uint32_t probe = 0u; probe < memo->capacity; ++probe) {
        uint32_t bucket = (uint32_t)(key_hash + probe) & mask;
        ClearraBuildUpCompletionMemoEntry *entry = &memo->entries[bucket];
        memo->probes++;
        if (probe + 1u > memo->max_probe_length) {
            memo->max_probe_length = probe + 1u;
        }
        if (memo->occupied_generations[bucket] != memo->generation) {
            return false;
        }
        if (entry_matches(entry, key, key_hash)) {
            memo->hits++;
            *out_completion_count = entry->completion_count;
            return true;
        }
    }
    return false;
}

void clearra_buildup_completion_memo_insert(
    ClearraBuildUpCompletionMemo *memo,
    const clr_buildup_memo_key *key,
    uint64_t completion_count) {
    if (memo == 0 || key == 0 || memo->entries == 0 ||
        memo->occupied_generations == 0 || memo->capacity == 0u ||
        !memo_identity_matches(memo, key)) {
        return;
    }
    if (memo->count >= memo->max_load) {
        memo->saturation_skips++;
        return;
    }

    uint64_t key_hash = clearra_buildup_memo_key_hash(key);
    uint32_t mask = memo->capacity - 1u;
    for (uint32_t probe = 0u; probe < memo->capacity; ++probe) {
        uint32_t bucket = (uint32_t)(key_hash + probe) & mask;
        ClearraBuildUpCompletionMemoEntry *entry = &memo->entries[bucket];
        memo->probes++;
        if (probe + 1u > memo->max_probe_length) {
            memo->max_probe_length = probe + 1u;
        }
        if (memo->occupied_generations[bucket] != memo->generation) {
            entry_store(entry, key, key_hash, completion_count);
            memo->occupied_generations[bucket] = memo->generation;
            memo->count++;
            memo->insertions++;
            return;
        }
        if (entry_matches(entry, key, key_hash)) {
            entry->completion_count = completion_count;
            return;
        }
    }
    memo->saturation_skips++;
}
