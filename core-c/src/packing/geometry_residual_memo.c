#include "geometry_residual_memo.h"

#include "clr_search_profile.h"

#include <stdlib.h>
#include <string.h>

#define CLEARRA_GEOMETRY_MEMO_MIN_CAPACITY 1024u
#define CLEARRA_GEOMETRY_MEMO_LOAD_NUMERATOR 7u
#define CLEARRA_GEOMETRY_MEMO_LOAD_DENOMINATOR 10u

static size_t next_power_of_two(size_t value) {
    size_t result = CLEARRA_GEOMETRY_MEMO_MIN_CAPACITY;
    while (result < value && result <= SIZE_MAX / 2u) {
        result *= 2u;
    }
    return result;
}

static uint64_t memo_hash(
    uint64_t remaining_cells,
    uint32_t packed_piece_counts) {
    uint64_t hash = remaining_cells ^
                    (packed_piece_counts * UINT64_C(0x9e3779b97f4a7c15));
    hash = (hash ^ (hash >> 30u)) * UINT64_C(0xbf58476d1ce4e5b9);
    hash = (hash ^ (hash >> 27u)) * UINT64_C(0x94d049bb133111eb);
    return hash ^ (hash >> 31u);
}

static size_t occupancy_word_count(size_t capacity) {
    return capacity / 64u + (capacity % 64u != 0u);
}

static bool allocation_layout(
    size_t capacity,
    size_t *out_entry_bytes,
    size_t *out_occupancy_bytes,
    size_t *out_total_bytes) {
    if (capacity < CLEARRA_GEOMETRY_MEMO_MIN_CAPACITY ||
        capacity > SIZE_MAX / sizeof(ClearraGeometryResidualMemoEntry)) {
        return false;
    }
    size_t entry_bytes =
        capacity * sizeof(ClearraGeometryResidualMemoEntry);
    size_t word_count = occupancy_word_count(capacity);
    if (word_count > SIZE_MAX / sizeof(uint64_t)) {
        return false;
    }
    size_t occupancy_bytes = word_count * sizeof(uint64_t);
    if (entry_bytes > SIZE_MAX - occupancy_bytes) {
        return false;
    }
    *out_entry_bytes = entry_bytes;
    *out_occupancy_bytes = occupancy_bytes;
    *out_total_bytes = entry_bytes + occupancy_bytes;
    return true;
}

static bool can_allocate_transient(
    const ClearraGeometryResidualMemo *memo,
    size_t bytes) {
    return bytes <= SIZE_MAX - memo->resident_bytes &&
           (memo->max_bytes == SIZE_MAX ||
            memo->resident_bytes + bytes <= memo->max_bytes);
}

static bool allocate_table(
    const ClearraGeometryResidualMemo *memo,
    size_t capacity,
    ClearraGeometryResidualMemoEntry **out_entries,
    uint64_t **out_occupied_words,
    size_t *out_allocation_bytes) {
    size_t entry_bytes = 0u;
    size_t occupancy_bytes = 0u;
    size_t allocation_bytes = 0u;
    if (!allocation_layout(
            capacity,
            &entry_bytes,
            &occupancy_bytes,
            &allocation_bytes) ||
        !can_allocate_transient(memo, allocation_bytes)) {
        return false;
    }
    ClearraGeometryResidualMemoEntry *entries =
        (ClearraGeometryResidualMemoEntry *)malloc(allocation_bytes);
    if (entries == 0) {
        return false;
    }
    uint64_t *occupied_words =
        (uint64_t *)((unsigned char *)entries + entry_bytes);
    memset(occupied_words, 0, occupancy_bytes);
    *out_entries = entries;
    *out_occupied_words = occupied_words;
    *out_allocation_bytes = allocation_bytes;
    return true;
}

static bool slot_occupied(
    const uint64_t *occupied_words,
    size_t slot) {
    return (occupied_words[slot / 64u] &
            (UINT64_C(1) << (slot % 64u))) != 0u;
}

static void mark_slot_occupied(uint64_t *occupied_words, size_t slot) {
    occupied_words[slot / 64u] |= UINT64_C(1) << (slot % 64u);
}

static void insert_entry(
    ClearraGeometryResidualMemoEntry *entries,
    uint64_t *occupied_words,
    size_t mask,
    ClearraGeometryResidualMemoEntry entry) {
    size_t slot =
        (size_t)memo_hash(entry.remaining_cells, entry.packed_piece_counts) &
        mask;
    while (slot_occupied(occupied_words, slot)) {
        slot = (slot + 1u) & mask;
    }
    entries[slot] = entry;
    mark_slot_occupied(occupied_words, slot);
}

static bool replace_with_empty_table(
    ClearraGeometryResidualMemo *memo,
    size_t capacity) {
    ClearraGeometryResidualMemoEntry *entries = 0;
    uint64_t *occupied_words = 0;
    size_t allocation_bytes = 0u;
    if (!allocate_table(
            memo,
            capacity,
            &entries,
            &occupied_words,
            &allocation_bytes)) {
        return false;
    }
    free(memo->entries);
    memo->entries = entries;
    memo->occupied_words = occupied_words;
    memo->capacity = capacity;
    memo->count = 0u;
    memo->mask = capacity - 1u;
    memo->allocation_bytes = allocation_bytes;
    memo->resident_bytes = allocation_bytes;
    return true;
}

static bool grow_table(ClearraGeometryResidualMemo *memo) {
    if (memo->capacity > SIZE_MAX / 2u) {
        return false;
    }
    size_t new_capacity = memo->capacity * 2u;
    ClearraGeometryResidualMemoEntry *new_entries = 0;
    uint64_t *new_occupied_words = 0;
    size_t new_allocation_bytes = 0u;
    if (!allocate_table(
            memo,
            new_capacity,
            &new_entries,
            &new_occupied_words,
            &new_allocation_bytes)) {
        return false;
    }
    size_t new_mask = new_capacity - 1u;
    for (size_t slot = 0u; slot < memo->capacity; ++slot) {
        if (slot_occupied(memo->occupied_words, slot)) {
            insert_entry(
                new_entries,
                new_occupied_words,
                new_mask,
                memo->entries[slot]);
        }
    }
    free(memo->entries);
    memo->entries = new_entries;
    memo->occupied_words = new_occupied_words;
    memo->capacity = new_capacity;
    memo->mask = new_mask;
    memo->allocation_bytes = new_allocation_bytes;
    memo->resident_bytes = new_allocation_bytes;
    return true;
}

void clearra_geometry_residual_memo_init(
    ClearraGeometryResidualMemo *memo,
    size_t expected_rows,
    size_t max_bytes) {
    if (memo == 0) {
        return;
    }
    *memo = (ClearraGeometryResidualMemo){.max_bytes = max_bytes};
    size_t desired = expected_rows > SIZE_MAX / 2u
                         ? SIZE_MAX
                         : expected_rows * 2u;
    size_t capacity = next_power_of_two(desired);
    while (capacity >= CLEARRA_GEOMETRY_MEMO_MIN_CAPACITY) {
        if (replace_with_empty_table(memo, capacity)) {
            return;
        }
        if (capacity == CLEARRA_GEOMETRY_MEMO_MIN_CAPACITY) {
            break;
        }
        capacity /= 2u;
    }
    memo->insertion_disabled = true;
}

void clearra_geometry_residual_memo_release(
    ClearraGeometryResidualMemo *memo) {
    if (memo == 0) {
        return;
    }
    free(memo->entries);
    *memo = (ClearraGeometryResidualMemo){0};
}

static ClearraGeometryResidualMemoEntry *find_entry(
    ClearraGeometryResidualMemo *memo,
    uint64_t remaining_cells,
    uint32_t packed_piece_counts,
    size_t *out_probe_length) {
    if (memo->entries == 0 || memo->capacity == 0u) {
        if (out_probe_length != 0) {
            *out_probe_length = 0u;
        }
        return 0;
    }
    uint64_t hash = memo_hash(remaining_cells, packed_piece_counts);
    size_t slot = (size_t)hash & memo->mask;
    for (size_t probe = 0u; probe < memo->capacity; ++probe) {
        if (!slot_occupied(memo->occupied_words, slot)) {
            if (out_probe_length != 0) {
                *out_probe_length = probe + 1u;
            }
            return 0;
        }
        ClearraGeometryResidualMemoEntry *entry = &memo->entries[slot];
        if (entry->remaining_cells == remaining_cells &&
            entry->packed_piece_counts == packed_piece_counts) {
            if (out_probe_length != 0) {
                *out_probe_length = probe + 1u;
            }
            return entry;
        }
        slot = (slot + 1u) & memo->mask;
    }
    if (out_probe_length != 0) {
        *out_probe_length = memo->capacity;
    }
    return 0;
}

bool clearra_geometry_residual_memo_lookup(
    ClearraGeometryResidualMemo *memo,
    uint64_t remaining_cells,
    uint32_t packed_piece_counts,
    uint32_t *out_suffix_family_ref) {
    if (memo == 0 || memo->entries == 0 || out_suffix_family_ref == 0) {
        return false;
    }
    memo->lookup_count++;
    clr_search_profile_count(
        CLR_PROFILE_PACKING_GEOMETRY_RESIDUAL_MEMO_LOOKUPS, 1u);
    size_t probe_length = 0u;
    ClearraGeometryResidualMemoEntry *entry = find_entry(
        memo, remaining_cells, packed_piece_counts, &probe_length);
    if (probe_length > memo->max_probe_length) {
        memo->max_probe_length = probe_length;
    }
    if (entry == 0) {
        return false;
    }
    memo->hit_count++;
    clr_search_profile_count(
        CLR_PROFILE_PACKING_GEOMETRY_RESIDUAL_MEMO_HITS, 1u);
    *out_suffix_family_ref = entry->suffix_family_ref;
    return true;
}

static bool ensure_insert_capacity(ClearraGeometryResidualMemo *memo) {
    if (memo->count * CLEARRA_GEOMETRY_MEMO_LOAD_DENOMINATOR <
        memo->capacity * CLEARRA_GEOMETRY_MEMO_LOAD_NUMERATOR) {
        return true;
    }
    if (!grow_table(memo)) {
        memo->insertion_disabled = true;
        return false;
    }
    return true;
}

void clearra_geometry_residual_memo_insert(
    ClearraGeometryResidualMemo *memo,
    uint64_t remaining_cells,
    uint32_t packed_piece_counts,
    uint32_t suffix_family_ref) {
    if (memo == 0 || memo->insertion_disabled || memo->entries == 0 ||
        find_entry(memo, remaining_cells, packed_piece_counts, 0) != 0 ||
        !ensure_insert_capacity(memo)) {
        return;
    }
    uint64_t hash = memo_hash(remaining_cells, packed_piece_counts);
    size_t slot = (size_t)hash & memo->mask;
    size_t probe_length = 1u;
    while (slot_occupied(memo->occupied_words, slot)) {
        slot = (slot + 1u) & memo->mask;
        probe_length++;
    }
    memo->entries[slot] = (ClearraGeometryResidualMemoEntry){
        .remaining_cells = remaining_cells,
        .packed_piece_counts = packed_piece_counts,
        .suffix_family_ref = suffix_family_ref,
    };
    mark_slot_occupied(memo->occupied_words, slot);
    memo->count++;
    if (probe_length > memo->max_probe_length) {
        memo->max_probe_length = probe_length;
    }
}
