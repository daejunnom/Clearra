#include "geometry_solution_family.h"

#include <stddef.h>
#include <stdlib.h>
#include <string.h>

#define CLEARRA_GEOMETRY_FAMILY_CHUNK_CAPACITY 4096u
#define CLEARRA_GEOMETRY_FAMILY_DIRECTORY_CAPACITY 256u
/* Covers every chunk addressable by the public 32-bit family reference. */
#define CLEARRA_GEOMETRY_FAMILY_DIRECTORY_INDEX_CAPACITY 4096u
#define CLEARRA_GEOMETRY_FAMILY_INTERN_INITIAL_CAPACITY 1024u

_Static_assert(
    (uint64_t)CLEARRA_GEOMETRY_FAMILY_DIRECTORY_INDEX_CAPACITY *
            CLEARRA_GEOMETRY_FAMILY_DIRECTORY_CAPACITY *
            CLEARRA_GEOMETRY_FAMILY_CHUNK_CAPACITY >=
        (uint64_t)UINT32_MAX - 2u,
    "family directory index must cover the full reference domain");

typedef struct ClearraGeometryFamilyInternEntry {
    uint64_t hash;
    ClearraGeometryFamilyRef reference;
    uint32_t reserved;
} ClearraGeometryFamilyInternEntry;

struct ClearraGeometryFamilyInternTable {
    size_t capacity;
    size_t count;
    size_t allocation_bytes;
    uint64_t occupied_words[1];
};

struct ClearraGeometryFamilyChunk {
    ClearraGeometryFamilyChunk *next;
    uint32_t count;
    uint32_t reserved;
    ClearraGeometryFamilyNode nodes[CLEARRA_GEOMETRY_FAMILY_CHUNK_CAPACITY];
};

struct ClearraGeometryFamilyDirectoryBlock {
    ClearraGeometryFamilyDirectoryBlock *next;
    uint32_t count;
    uint32_t reserved;
    ClearraGeometryFamilyChunk
        *chunks[CLEARRA_GEOMETRY_FAMILY_DIRECTORY_CAPACITY];
};

static bool can_allocate(
    const ClearraGeometrySolutionFamily *family,
    size_t bytes) {
    return bytes <= SIZE_MAX - family->resident_bytes &&
           (family->max_bytes == SIZE_MAX ||
            family->resident_bytes + bytes <= family->max_bytes);
}

void clearra_geometry_solution_family_init(
    ClearraGeometrySolutionFamily *family,
    size_t max_bytes) {
    if (family != 0) {
        *family = (ClearraGeometrySolutionFamily){.max_bytes = max_bytes};
    }
}

void clearra_geometry_solution_family_release(
    ClearraGeometrySolutionFamily *family) {
    if (family == 0) {
        return;
    }
    ClearraGeometryFamilyChunk *chunk = family->chunks;
    while (chunk != 0) {
        ClearraGeometryFamilyChunk *next = chunk->next;
        free(chunk);
        chunk = next;
    }
    ClearraGeometryFamilyDirectoryBlock *directory = family->directories;
    while (directory != 0) {
        ClearraGeometryFamilyDirectoryBlock *next = directory->next;
        free(directory);
        directory = next;
    }
    free(family->directory_index);
    free(family->intern_table);
    *family = (ClearraGeometrySolutionFamily){0};
}

void clearra_geometry_solution_family_checkpoint_begin(
    ClearraGeometrySolutionFamily *family,
    ClearraGeometrySolutionFamilyCheckpoint *checkpoint) {
    if (family == 0 || checkpoint == 0) {
        return;
    }
    *checkpoint = (ClearraGeometrySolutionFamilyCheckpoint){
        .tail = family->tail,
        .directory_tail = family->directory_tail,
        .directory_index = family->directory_index,
        .resident_bytes = family->resident_bytes,
        .node_count = family->node_count,
        .directory_block_count = family->directory_block_count,
        .tail_count = family->tail == 0 ? 0u : family->tail->count,
        .directory_count = family->directory_tail == 0
            ? 0u
            : family->directory_tail->count,
        .allocation_failed = family->allocation_failed,
        .interning_disabled = family->interning_disabled,
    };
    family->interning_disabled = true;
}

void clearra_geometry_solution_family_checkpoint_commit(
    ClearraGeometrySolutionFamily *family,
    const ClearraGeometrySolutionFamilyCheckpoint *checkpoint) {
    if (family != 0 && checkpoint != 0) {
        family->interning_disabled = checkpoint->interning_disabled;
    }
}

void clearra_geometry_solution_family_checkpoint_rollback(
    ClearraGeometrySolutionFamily *family,
    const ClearraGeometrySolutionFamilyCheckpoint *checkpoint) {
    if (family == 0 || checkpoint == 0) {
        return;
    }

    ClearraGeometryFamilyChunk *chunk = checkpoint->tail == 0
        ? family->chunks
        : checkpoint->tail->next;
    while (chunk != 0) {
        ClearraGeometryFamilyChunk *next = chunk->next;
        free(chunk);
        chunk = next;
    }
    if (checkpoint->tail == 0) {
        family->chunks = 0;
    } else {
        checkpoint->tail->next = 0;
        checkpoint->tail->count = checkpoint->tail_count;
    }
    family->tail = checkpoint->tail;

    ClearraGeometryFamilyDirectoryBlock *directory =
        checkpoint->directory_tail == 0
        ? family->directories
        : checkpoint->directory_tail->next;
    while (directory != 0) {
        ClearraGeometryFamilyDirectoryBlock *next = directory->next;
        free(directory);
        directory = next;
    }
    if (checkpoint->directory_tail == 0) {
        family->directories = 0;
    } else {
        checkpoint->directory_tail->next = 0;
        checkpoint->directory_tail->count = checkpoint->directory_count;
    }
    family->directory_tail = checkpoint->directory_tail;
    if (checkpoint->directory_index == 0) {
        free(family->directory_index);
        family->directory_index = 0;
    } else {
        for (uint32_t index = checkpoint->directory_block_count;
             index < family->directory_block_count;
             ++index) {
            family->directory_index[index] = 0;
        }
        family->directory_index = checkpoint->directory_index;
    }
    family->directory_block_count = checkpoint->directory_block_count;
    family->resident_bytes = checkpoint->resident_bytes;
    family->node_count = checkpoint->node_count;
    family->allocation_failed = checkpoint->allocation_failed;
    family->interning_disabled = checkpoint->interning_disabled;
}

static uint64_t mix_hash(uint64_t hash, uint64_t value) {
    hash ^= value + UINT64_C(0x9e3779b97f4a7c15) + (hash << 6u) +
            (hash >> 2u);
    hash ^= hash >> 30u;
    hash *= UINT64_C(0xbf58476d1ce4e5b9);
    hash ^= hash >> 27u;
    hash *= UINT64_C(0x94d049bb133111eb);
    return hash ^ (hash >> 31u);
}

static uint64_t node_hash(
    uint8_t kind,
    uint32_t left,
    uint32_t right,
    uint32_t row_id) {
    uint64_t hash = mix_hash(UINT64_C(0x6a09e667f3bcc909), kind);
    hash = mix_hash(hash, left);
    hash = mix_hash(hash, right);
    hash = mix_hash(hash, row_id);
    return hash == 0u ? UINT64_C(1) : hash;
}

static bool node_matches(
    const ClearraGeometrySolutionFamily *family,
    ClearraGeometryFamilyRef reference,
    uint8_t kind,
    uint32_t left,
    uint32_t right,
    uint32_t row_id) {
    const ClearraGeometryFamilyNode *node =
        clearra_geometry_solution_family_node(family, reference);
    return node != 0 && node->kind == kind && node->left == left &&
           node->right == right && node->row_id == row_id;
}

static size_t intern_occupied_word_count(size_t capacity) {
    return capacity / 64u + (capacity % 64u != 0u ? 1u : 0u);
}

static ClearraGeometryFamilyInternEntry *intern_entries(
    ClearraGeometryFamilyInternTable *table) {
    return (ClearraGeometryFamilyInternEntry *)(
        table->occupied_words + intern_occupied_word_count(table->capacity));
}

static const ClearraGeometryFamilyInternEntry *intern_entries_const(
    const ClearraGeometryFamilyInternTable *table) {
    return (const ClearraGeometryFamilyInternEntry *)(
        table->occupied_words + intern_occupied_word_count(table->capacity));
}

static bool intern_entry_occupied(
    const ClearraGeometryFamilyInternTable *table,
    size_t index) {
    return (table->occupied_words[index / 64u] &
            (UINT64_C(1) << (index % 64u))) != 0u;
}

static void mark_intern_entry_occupied(
    ClearraGeometryFamilyInternTable *table,
    size_t index) {
    table->occupied_words[index / 64u] |=
        UINT64_C(1) << (index % 64u);
}

static ClearraGeometryFamilyRef find_interned_node(
    const ClearraGeometrySolutionFamily *family,
    uint64_t hash,
    uint8_t kind,
    uint32_t left,
    uint32_t right,
    uint32_t row_id) {
    const ClearraGeometryFamilyInternTable *table = family->intern_table;
    if (table == 0) {
        return CLEARRA_GEOMETRY_FAMILY_INVALID;
    }
    const ClearraGeometryFamilyInternEntry *entries =
        intern_entries_const(table);
    size_t index = (size_t)hash & (table->capacity - 1u);
    for (size_t probe = 0u; probe < table->capacity; ++probe) {
        const ClearraGeometryFamilyInternEntry *entry = &entries[index];
        if (!intern_entry_occupied(table, index)) {
            break;
        }
        if (entry->hash == hash &&
            node_matches(
                family,
                entry->reference,
                kind,
                left,
                right,
                row_id)) {
            return entry->reference;
        }
        index = (index + 1u) & (table->capacity - 1u);
    }
    return CLEARRA_GEOMETRY_FAMILY_INVALID;
}

static size_t intern_table_bytes(size_t capacity) {
    if (capacity == 0u) {
        return SIZE_MAX;
    }
    size_t occupied_words = intern_occupied_word_count(capacity);
    size_t header_bytes = offsetof(
        ClearraGeometryFamilyInternTable, occupied_words);
    if (occupied_words > (SIZE_MAX - header_bytes) / sizeof(uint64_t)) {
        return SIZE_MAX;
    }
    size_t bytes = header_bytes + occupied_words * sizeof(uint64_t);
    if (capacity >
        (SIZE_MAX - bytes) / sizeof(ClearraGeometryFamilyInternEntry)) {
        return SIZE_MAX;
    }
    return bytes + capacity * sizeof(ClearraGeometryFamilyInternEntry);
}

static void initialize_intern_entries(
    ClearraGeometryFamilyInternTable *table) {
    memset(
        table->occupied_words,
        0,
        intern_occupied_word_count(table->capacity) * sizeof(uint64_t));
}

static void insert_intern_entry(
    ClearraGeometryFamilyInternTable *table,
    uint64_t hash,
    ClearraGeometryFamilyRef reference) {
    ClearraGeometryFamilyInternEntry *entries = intern_entries(table);
    size_t index = (size_t)hash & (table->capacity - 1u);
    while (intern_entry_occupied(table, index)) {
        index = (index + 1u) & (table->capacity - 1u);
    }
    entries[index] = (ClearraGeometryFamilyInternEntry){
        .hash = hash,
        .reference = reference,
    };
    mark_intern_entry_occupied(table, index);
    table->count++;
}

static ClearraGeometryFamilyInternTable *grow_intern_table(
    ClearraGeometrySolutionFamily *family) {
    ClearraGeometryFamilyInternTable *old_table = family->intern_table;
    size_t capacity = old_table == 0
        ? CLEARRA_GEOMETRY_FAMILY_INTERN_INITIAL_CAPACITY
        : old_table->capacity <= SIZE_MAX / 2u
            ? old_table->capacity * 2u
            : 0u;
    size_t bytes = capacity == 0u ? SIZE_MAX : intern_table_bytes(capacity);
    if (bytes == SIZE_MAX || !can_allocate(family, bytes)) {
        family->interning_disabled = true;
        return 0;
    }
    ClearraGeometryFamilyInternTable *table =
        (ClearraGeometryFamilyInternTable *)malloc(bytes);
    if (table == 0) {
        family->interning_disabled = true;
        return 0;
    }
    table->capacity = capacity;
    table->count = 0u;
    table->allocation_bytes = bytes;
    initialize_intern_entries(table);

    if (old_table != 0) {
        const ClearraGeometryFamilyInternEntry *old_entries =
            intern_entries_const(old_table);
        for (size_t index = 0u; index < old_table->capacity; ++index) {
            if (intern_entry_occupied(old_table, index)) {
                insert_intern_entry(
                    table,
                    old_entries[index].hash,
                    old_entries[index].reference);
            }
        }
    }

    family->intern_table = table;
    family->resident_bytes += bytes;
    if (old_table != 0) {
        family->resident_bytes -= old_table->allocation_bytes;
        free(old_table);
    }
    return table;
}

static void intern_node(
    ClearraGeometrySolutionFamily *family,
    uint64_t hash,
    ClearraGeometryFamilyRef reference) {
    if (family->interning_disabled) {
        return;
    }
    ClearraGeometryFamilyInternTable *table = family->intern_table;
    if (table == 0 ||
        table->count + 1u > (table->capacity * 7u) / 10u) {
        table = grow_intern_table(family);
        if (table == 0) {
            return;
        }
    }
    insert_intern_entry(table, hash, reference);
}

static bool append_chunk_to_directory(
    ClearraGeometrySolutionFamily *family,
    ClearraGeometryFamilyChunk *chunk) {
    ClearraGeometryFamilyDirectoryBlock *directory = family->directory_tail;
    if (directory == 0 ||
        directory->count == CLEARRA_GEOMETRY_FAMILY_DIRECTORY_CAPACITY) {
        if (family->directory_block_count >=
            CLEARRA_GEOMETRY_FAMILY_DIRECTORY_INDEX_CAPACITY) {
            return false;
        }
        if (family->directory_index == 0) {
            size_t index_bytes =
                (size_t)CLEARRA_GEOMETRY_FAMILY_DIRECTORY_INDEX_CAPACITY *
                sizeof(*family->directory_index);
            if (!can_allocate(family, index_bytes)) {
                return false;
            }
            family->directory_index =
                (ClearraGeometryFamilyDirectoryBlock **)malloc(index_bytes);
            if (family->directory_index == 0) {
                return false;
            }
            memset(family->directory_index, 0, index_bytes);
            family->resident_bytes += index_bytes;
        }
        if (!can_allocate(family, sizeof(*directory))) {
            return false;
        }
        directory = (ClearraGeometryFamilyDirectoryBlock *)malloc(
            sizeof(*directory));
        if (directory == 0) {
            return false;
        }
        directory->next = 0;
        directory->count = 0u;
        directory->reserved = 0u;
        if (family->directory_tail == 0) {
            family->directories = directory;
        } else {
            family->directory_tail->next = directory;
        }
        family->directory_tail = directory;
        family->directory_index[family->directory_block_count++] = directory;
        family->resident_bytes += sizeof(*directory);
    }
    directory->chunks[directory->count++] = chunk;
    return true;
}

static ClearraGeometryFamilyNode *allocate_node(
    ClearraGeometrySolutionFamily *family,
    ClearraGeometryFamilyRef *out_reference) {
    if (family == 0 || out_reference == 0 || family->allocation_failed ||
        family->node_count > UINT32_MAX - 2u) {
        if (family != 0) {
            family->allocation_failed = true;
        }
        return 0;
    }
    ClearraGeometryFamilyChunk *chunk = family->tail;
    if (chunk == 0 ||
        chunk->count == CLEARRA_GEOMETRY_FAMILY_CHUNK_CAPACITY) {
        size_t required = sizeof(*chunk);
        ClearraGeometryFamilyDirectoryBlock *directory = family->directory_tail;
        if (directory == 0 ||
            directory->count == CLEARRA_GEOMETRY_FAMILY_DIRECTORY_CAPACITY) {
            required = required > SIZE_MAX - sizeof(*directory)
                ? SIZE_MAX
                : required + sizeof(*directory);
            if (family->directory_index == 0) {
                size_t index_bytes =
                    (size_t)CLEARRA_GEOMETRY_FAMILY_DIRECTORY_INDEX_CAPACITY *
                    sizeof(*family->directory_index);
                required = required > SIZE_MAX - index_bytes
                    ? SIZE_MAX
                    : required + index_bytes;
            }
        }
        if (!can_allocate(family, required)) {
            family->allocation_failed = true;
            return 0;
        }
        chunk = (ClearraGeometryFamilyChunk *)malloc(sizeof(*chunk));
        if (chunk == 0) {
            family->allocation_failed = true;
            return 0;
        }
        chunk->next = 0;
        chunk->count = 0u;
        chunk->reserved = 0u;
        if (!append_chunk_to_directory(family, chunk)) {
            free(chunk);
            family->allocation_failed = true;
            return 0;
        }
        if (family->tail == 0) {
            family->chunks = chunk;
        } else {
            family->tail->next = chunk;
        }
        family->tail = chunk;
        family->resident_bytes += sizeof(*chunk);
    }
    *out_reference = family->node_count + 2u;
    family->node_count++;
    return &chunk->nodes[chunk->count++];
}

ClearraGeometryFamilyRef clearra_geometry_solution_family_append(
    ClearraGeometrySolutionFamily *family,
    uint32_t row_id,
    ClearraGeometryFamilyRef suffix) {
    if (family == 0 || suffix == CLEARRA_GEOMETRY_FAMILY_INVALID) {
        return CLEARRA_GEOMETRY_FAMILY_INVALID;
    }
    uint64_t hash = node_hash(
        CLEARRA_GEOMETRY_FAMILY_APPEND, suffix, 0u, row_id);
    ClearraGeometryFamilyRef existing = find_interned_node(
        family,
        hash,
        CLEARRA_GEOMETRY_FAMILY_APPEND,
        suffix,
        0u,
        row_id);
    if (existing != CLEARRA_GEOMETRY_FAMILY_INVALID) {
        return existing;
    }
    ClearraGeometryFamilyRef reference = CLEARRA_GEOMETRY_FAMILY_INVALID;
    ClearraGeometryFamilyNode *node = allocate_node(family, &reference);
    if (node == 0) {
        return CLEARRA_GEOMETRY_FAMILY_INVALID;
    }
    *node = (ClearraGeometryFamilyNode){
        .left = suffix,
        .row_id = row_id,
        .kind = CLEARRA_GEOMETRY_FAMILY_APPEND,
    };
    intern_node(family, hash, reference);
    return reference;
}

ClearraGeometryFamilyRef clearra_geometry_solution_family_union(
    ClearraGeometrySolutionFamily *family,
    ClearraGeometryFamilyRef left,
    ClearraGeometryFamilyRef right) {
    if (family == 0) {
        return CLEARRA_GEOMETRY_FAMILY_INVALID;
    }
    if (left == CLEARRA_GEOMETRY_FAMILY_INVALID) {
        return right;
    }
    if (right == CLEARRA_GEOMETRY_FAMILY_INVALID) {
        return left;
    }
    if (left == right) {
        return left;
    }
    if (right < left) {
        ClearraGeometryFamilyRef swap = left;
        left = right;
        right = swap;
    }
    uint64_t hash = node_hash(
        CLEARRA_GEOMETRY_FAMILY_UNION, left, right, 0u);
    ClearraGeometryFamilyRef existing = find_interned_node(
        family,
        hash,
        CLEARRA_GEOMETRY_FAMILY_UNION,
        left,
        right,
        0u);
    if (existing != CLEARRA_GEOMETRY_FAMILY_INVALID) {
        return existing;
    }
    ClearraGeometryFamilyRef reference = CLEARRA_GEOMETRY_FAMILY_INVALID;
    ClearraGeometryFamilyNode *node = allocate_node(family, &reference);
    if (node == 0) {
        return CLEARRA_GEOMETRY_FAMILY_INVALID;
    }
    *node = (ClearraGeometryFamilyNode){
        .left = left,
        .right = right,
        .kind = CLEARRA_GEOMETRY_FAMILY_UNION,
    };
    intern_node(family, hash, reference);
    return reference;
}

ClearraGeometryFamilyRef clearra_geometry_solution_family_product(
    ClearraGeometrySolutionFamily *family,
    ClearraGeometryFamilyRef left,
    ClearraGeometryFamilyRef right) {
    if (family == 0 || left == CLEARRA_GEOMETRY_FAMILY_INVALID ||
        right == CLEARRA_GEOMETRY_FAMILY_INVALID) {
        return CLEARRA_GEOMETRY_FAMILY_INVALID;
    }
    if (left == CLEARRA_GEOMETRY_FAMILY_EMPTY) {
        return right;
    }
    if (right == CLEARRA_GEOMETRY_FAMILY_EMPTY) {
        return left;
    }
    if (right < left) {
        ClearraGeometryFamilyRef swap = left;
        left = right;
        right = swap;
    }
    uint64_t hash = node_hash(
        CLEARRA_GEOMETRY_FAMILY_PRODUCT, left, right, 0u);
    ClearraGeometryFamilyRef existing = find_interned_node(
        family,
        hash,
        CLEARRA_GEOMETRY_FAMILY_PRODUCT,
        left,
        right,
        0u);
    if (existing != CLEARRA_GEOMETRY_FAMILY_INVALID) {
        return existing;
    }
    ClearraGeometryFamilyRef reference = CLEARRA_GEOMETRY_FAMILY_INVALID;
    ClearraGeometryFamilyNode *node = allocate_node(family, &reference);
    if (node == 0) {
        return CLEARRA_GEOMETRY_FAMILY_INVALID;
    }
    *node = (ClearraGeometryFamilyNode){
        .left = left,
        .right = right,
        .kind = CLEARRA_GEOMETRY_FAMILY_PRODUCT,
    };
    intern_node(family, hash, reference);
    return reference;
}

const ClearraGeometryFamilyNode *clearra_geometry_solution_family_node(
    const ClearraGeometrySolutionFamily *family,
    ClearraGeometryFamilyRef reference) {
    if (family == 0 || reference < 2u || reference - 2u >= family->node_count) {
        return 0;
    }
    uint32_t node_index = reference - 2u;
    uint32_t chunk_index =
        node_index / CLEARRA_GEOMETRY_FAMILY_CHUNK_CAPACITY;
    uint32_t node_offset =
        node_index % CLEARRA_GEOMETRY_FAMILY_CHUNK_CAPACITY;
    uint32_t directory_index =
        chunk_index / CLEARRA_GEOMETRY_FAMILY_DIRECTORY_CAPACITY;
    uint32_t directory_offset =
        chunk_index % CLEARRA_GEOMETRY_FAMILY_DIRECTORY_CAPACITY;
    const ClearraGeometryFamilyDirectoryBlock *directory =
        family->directory_index == 0 ||
                directory_index >= family->directory_block_count
            ? 0
            : family->directory_index[directory_index];
    if (directory == 0 || directory_offset >= directory->count) {
        return 0;
    }
    const ClearraGeometryFamilyChunk *chunk =
        directory->chunks[directory_offset];
    return chunk == 0 || node_offset >= chunk->count
        ? 0
        : &chunk->nodes[node_offset];
}
