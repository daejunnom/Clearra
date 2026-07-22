#include "geometry_component_solution_table.h"

#include <stdlib.h>
#include <string.h>

#define CLEARRA_COMPONENT_TABLE_MIN_BUCKETS 64u
#define CLEARRA_COMPONENT_TABLE_CHUNK_CAPACITY 128u
#define CLEARRA_COMPONENT_TABLE_LOAD_NUMERATOR 3u
#define CLEARRA_COMPONENT_TABLE_LOAD_DENOMINATOR 4u

struct ClearraGeometryComponentSolutionChunk {
    ClearraGeometryComponentSolutionChunk *next;
    uint32_t count;
    uint32_t reserved;
    ClearraGeometryComponentSolutionEntry
        entries[CLEARRA_COMPONENT_TABLE_CHUNK_CAPACITY];
};

static size_t next_power_of_two(size_t value) {
    size_t result = CLEARRA_COMPONENT_TABLE_MIN_BUCKETS;
    while (result < value && result <= SIZE_MAX / 2u) {
        result *= 2u;
    }
    return result;
}

static uint64_t signature_hash(uint32_t signature) {
    uint64_t hash = (uint64_t)signature + UINT64_C(0x9e3779b97f4a7c15);
    hash = (hash ^ (hash >> 30u)) * UINT64_C(0xbf58476d1ce4e5b9);
    hash = (hash ^ (hash >> 27u)) * UINT64_C(0x94d049bb133111eb);
    return hash ^ (hash >> 31u);
}

static bool can_allocate(
    const ClearraGeometryComponentSolutionTable *table,
    size_t bytes) {
    return bytes <= SIZE_MAX - table->resident_bytes &&
           (table->max_bytes == SIZE_MAX ||
            table->resident_bytes + bytes <= table->max_bytes);
}

bool clearra_geometry_component_solution_table_init(
    ClearraGeometryComponentSolutionTable *table,
    size_t expected_signatures,
    size_t max_bytes) {
    if (table == 0) {
        return false;
    }
    *table = (ClearraGeometryComponentSolutionTable){.max_bytes = max_bytes};
    size_t desired = expected_signatures > SIZE_MAX / 2u
        ? SIZE_MAX
        : expected_signatures * 2u;
    size_t bucket_count = next_power_of_two(desired);
    if (bucket_count > SIZE_MAX / sizeof(*table->buckets)) {
        table->allocation_failed = true;
        return false;
    }
    size_t bytes = bucket_count * sizeof(*table->buckets);
    if (!can_allocate(table, bytes)) {
        table->allocation_failed = true;
        return false;
    }
    table->buckets =
        (ClearraGeometryComponentSolutionEntry **)malloc(bytes);
    if (table->buckets == 0) {
        table->allocation_failed = true;
        return false;
    }
    memset(table->buckets, 0, bytes);
    table->bucket_count = bucket_count;
    table->resident_bytes = bytes;
    return true;
}

void clearra_geometry_component_solution_table_release(
    ClearraGeometryComponentSolutionTable *table) {
    if (table == 0) {
        return;
    }
    ClearraGeometryComponentSolutionChunk *chunk = table->chunks;
    while (chunk != 0) {
        ClearraGeometryComponentSolutionChunk *next = chunk->next;
        free(chunk);
        chunk = next;
    }
    free(table->buckets);
    *table = (ClearraGeometryComponentSolutionTable){0};
}

static ClearraGeometryComponentSolutionEntry *allocate_entry(
    ClearraGeometryComponentSolutionTable *table) {
    ClearraGeometryComponentSolutionChunk *chunk = table->tail;
    if (chunk == 0 || chunk->count == CLEARRA_COMPONENT_TABLE_CHUNK_CAPACITY) {
        if (!can_allocate(table, sizeof(*chunk))) {
            table->allocation_failed = true;
            return 0;
        }
        chunk = (ClearraGeometryComponentSolutionChunk *)malloc(sizeof(*chunk));
        if (chunk == 0) {
            table->allocation_failed = true;
            return 0;
        }
        chunk->next = 0;
        chunk->count = 0u;
        chunk->reserved = 0u;
        if (table->tail == 0) {
            table->chunks = chunk;
        } else {
            table->tail->next = chunk;
        }
        table->tail = chunk;
        table->resident_bytes += sizeof(*chunk);
    }
    return &chunk->entries[chunk->count++];
}

static bool grow_bucket_index(ClearraGeometryComponentSolutionTable *table) {
    if (table->bucket_growth_disabled) {
        return false;
    }
    if (table->bucket_count > SIZE_MAX / 2u) {
        table->bucket_growth_disabled = true;
        return false;
    }
    size_t bucket_count = table->bucket_count * 2u;
    if (bucket_count > SIZE_MAX / sizeof(*table->buckets)) {
        table->bucket_growth_disabled = true;
        return false;
    }
    size_t bytes = bucket_count * sizeof(*table->buckets);
    if (!can_allocate(table, bytes)) {
        table->bucket_growth_disabled = true;
        return false;
    }
    ClearraGeometryComponentSolutionEntry **buckets =
        (ClearraGeometryComponentSolutionEntry **)malloc(bytes);
    if (buckets == 0) {
        table->bucket_growth_disabled = true;
        return false;
    }
    memset(buckets, 0, bytes);

    for (ClearraGeometryComponentSolutionChunk *chunk = table->chunks;
         chunk != 0;
         chunk = chunk->next) {
        for (uint32_t index = 0u; index < chunk->count; ++index) {
            ClearraGeometryComponentSolutionEntry *entry = &chunk->entries[index];
            size_t bucket = (size_t)signature_hash(entry->piece_count_signature) &
                            (bucket_count - 1u);
            entry->next_in_bucket = buckets[bucket];
            buckets[bucket] = entry;
        }
    }

    size_t old_bytes = table->bucket_count * sizeof(*table->buckets);
    ClearraGeometryComponentSolutionEntry **old_buckets = table->buckets;
    table->buckets = buckets;
    table->bucket_count = bucket_count;
    table->resident_bytes = table->resident_bytes + bytes - old_bytes;
    free(old_buckets);
    return true;
}

static void grow_bucket_index_if_useful(
    ClearraGeometryComponentSolutionTable *table) {
    size_t load_limit = table->bucket_count -
        table->bucket_count *
            (CLEARRA_COMPONENT_TABLE_LOAD_DENOMINATOR -
             CLEARRA_COMPONENT_TABLE_LOAD_NUMERATOR) /
            CLEARRA_COMPONENT_TABLE_LOAD_DENOMINATOR;
    if (!table->bucket_growth_disabled &&
        table->entry_count >= load_limit) {
        (void)grow_bucket_index(table);
    }
}

ClearraGeometryComponentInsertStatus
clearra_geometry_component_solution_table_insert(
    ClearraGeometryComponentSolutionTable *table,
    ClearraGeometrySolutionFamily *family,
    uint32_t piece_count_signature,
    ClearraGeometryFamilyRef family_ref) {
    if (table == 0 || family == 0 || table->bucket_count == 0u ||
        table->allocation_failed ||
        family_ref == CLEARRA_GEOMETRY_FAMILY_INVALID) {
        return CLEARRA_GEOMETRY_COMPONENT_TABLE_UNAVAILABLE;
    }
    size_t bucket = (size_t)signature_hash(piece_count_signature) &
                    (table->bucket_count - 1u);
    for (ClearraGeometryComponentSolutionEntry *entry = table->buckets[bucket];
         entry != 0;
         entry = entry->next_in_bucket) {
        if (entry->piece_count_signature != piece_count_signature) {
            continue;
        }
        ClearraGeometryFamilyRef merged = clearra_geometry_solution_family_union(
            family, entry->family_ref, family_ref);
        if (merged == CLEARRA_GEOMETRY_FAMILY_INVALID) {
            return CLEARRA_GEOMETRY_COMPONENT_FAMILY_UNAVAILABLE;
        }
        entry->family_ref = merged;
        return CLEARRA_GEOMETRY_COMPONENT_INSERT_OK;
    }
    grow_bucket_index_if_useful(table);
    bucket = (size_t)signature_hash(piece_count_signature) &
             (table->bucket_count - 1u);
    ClearraGeometryComponentSolutionEntry *entry = allocate_entry(table);
    if (entry == 0) {
        return CLEARRA_GEOMETRY_COMPONENT_TABLE_UNAVAILABLE;
    }
    *entry = (ClearraGeometryComponentSolutionEntry){
        .next_in_bucket = table->buckets[bucket],
        .piece_count_signature = piece_count_signature,
        .family_ref = family_ref,
    };
    table->buckets[bucket] = entry;
    table->entry_count++;
    return CLEARRA_GEOMETRY_COMPONENT_INSERT_OK;
}

void clearra_geometry_component_solution_iterator_begin(
    const ClearraGeometryComponentSolutionTable *table,
    ClearraGeometryComponentSolutionIterator *iterator) {
    if (iterator != 0) {
        *iterator = (ClearraGeometryComponentSolutionIterator){
            .chunk = table == 0 ? 0 : table->chunks,
        };
    }
}

const ClearraGeometryComponentSolutionEntry *
clearra_geometry_component_solution_iterator_next(
    ClearraGeometryComponentSolutionIterator *iterator) {
    if (iterator == 0) {
        return 0;
    }
    while (iterator->chunk != 0 &&
           iterator->index >= iterator->chunk->count) {
        iterator->chunk = iterator->chunk->next;
        iterator->index = 0u;
    }
    return iterator->chunk == 0
        ? 0
        : &iterator->chunk->entries[iterator->index++];
}
