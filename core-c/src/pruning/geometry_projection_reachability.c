#include "geometry_projection_reachability.h"

#include "../cache/cache_identity.h"
#include "../packing/geometry_exact_cover_internal.h"

#include <stdbool.h>
#include <stdlib.h>
#include <string.h>

#define CLEARRA_PROJECTION_CACHE_CHUNK_CAPACITY 16u
#define CLEARRA_PROJECTION_INITIAL_BUCKET_COUNT 64u
#define CLEARRA_PROJECTION_SET_INITIAL_CAPACITY 64u
#define CLEARRA_PROJECTION_REACHABILITY_PROOF_VERSION UINT64_C(1)

typedef struct ClearraProjectionSetSegment ClearraProjectionSetSegment;

struct ClearraProjectionSetSegment {
    ClearraProjectionSetSegment *next;
    ClearraGeometryColumnSignature *signatures;
    uint64_t *occupied_words;
    size_t bytes;
    uint32_t capacity;
    uint32_t count;
};

typedef struct ClearraProjectionSet {
    ClearraProjectionSetSegment *segments;
    ClearraProjectionSetSegment *active;
    size_t bytes;
    uint32_t count;
} ClearraProjectionSet;

struct ClearraGeometryProjectionCacheEntry {
    ClearraGeometryProjectionCacheEntry *next_bucket;
    ClearraGeometryColumnSignature *signatures;
    uint64_t signature_digest;
    uint64_t piece_count_key;
    uint32_t signature_count;
};

struct ClearraGeometryProjectionCacheChunk {
    ClearraGeometryProjectionCacheChunk *next;
    uint32_t count;
    ClearraGeometryProjectionCacheEntry
        entries[CLEARRA_PROJECTION_CACHE_CHUNK_CAPACITY];
};

struct ClearraGeometryProjectionBucketTable {
    ClearraGeometryProjectionCacheEntry **buckets;
    size_t bytes;
    uint32_t bucket_count;
    uint32_t entry_count;
};

static bool signature_equal(
    ClearraGeometryColumnSignature left,
    ClearraGeometryColumnSignature right) {
    return left.low == right.low && left.high == right.high;
}

static uint64_t signature_hash(ClearraGeometryColumnSignature signature) {
    uint64_t hash = clearra_cache_key_mix_u64(
        UINT64_C(1469598103934665603), signature.low);
    return clearra_cache_key_mix_u64(hash, signature.high);
}

static int compare_signatures(const void *left_ptr, const void *right_ptr) {
    const ClearraGeometryColumnSignature *left =
        (const ClearraGeometryColumnSignature *)left_ptr;
    const ClearraGeometryColumnSignature *right =
        (const ClearraGeometryColumnSignature *)right_ptr;
    if (left->high != right->high) {
        return left->high < right->high ? -1 : 1;
    }
    if (left->low != right->low) {
        return left->low < right->low ? -1 : 1;
    }
    return 0;
}

static bool cache_can_add(
    const ClearraGeometryProjectionReachabilityCache *cache,
    size_t bytes) {
    return bytes <= SIZE_MAX - cache->resident_bytes &&
           (cache->max_resident_bytes == SIZE_MAX ||
            cache->resident_bytes + bytes <= cache->max_resident_bytes);
}

static void *cache_allocate(
    ClearraGeometryProjectionReachabilityCache *cache,
    size_t bytes) {
    if (bytes == 0u || !cache_can_add(cache, bytes)) {
        return 0;
    }
    void *value = malloc(bytes);
    if (value != 0) {
        cache->resident_bytes += bytes;
    }
    return value;
}

void clearra_geometry_projection_cache_init(
    ClearraGeometryProjectionReachabilityCache *cache,
    size_t max_resident_bytes) {
    if (cache == 0) {
        return;
    }
    *cache = (ClearraGeometryProjectionReachabilityCache){
        .max_resident_bytes = max_resident_bytes,
    };
}

void clearra_geometry_projection_cache_release(
    ClearraGeometryProjectionReachabilityCache *cache) {
    if (cache == 0) {
        return;
    }
    ClearraGeometryProjectionCacheChunk *chunk = cache->chunks;
    while (chunk != 0) {
        for (uint32_t index = 0u; index < chunk->count; ++index) {
            free(chunk->entries[index].signatures);
        }
        ClearraGeometryProjectionCacheChunk *next = chunk->next;
        free(chunk);
        chunk = next;
    }
    free(cache->bucket_table);
    *cache = (ClearraGeometryProjectionReachabilityCache){0};
}

static uint32_t cache_bucket(
    uint64_t piece_count_key,
    uint32_t bucket_count) {
    uint64_t hash = clearra_cache_key_mix_u64(
        UINT64_C(1469598103934665603), piece_count_key);
    return (uint32_t)(hash & (bucket_count - 1u));
}

static ClearraGeometryProjectionCacheEntry *cache_find(
    const ClearraGeometryProjectionReachabilityCache *cache,
    uint64_t piece_count_key) {
    const ClearraGeometryProjectionBucketTable *table = cache->bucket_table;
    if (table == 0) {
        return 0;
    }
    uint32_t bucket = cache_bucket(piece_count_key, table->bucket_count);
    for (ClearraGeometryProjectionCacheEntry *entry = table->buckets[bucket];
         entry != 0;
         entry = entry->next_bucket) {
        if (entry->piece_count_key == piece_count_key) {
            return entry;
        }
    }
    return 0;
}

static bool cache_replace_bucket_table(
    ClearraGeometryProjectionReachabilityCache *cache,
    uint32_t bucket_count) {
    if (bucket_count < CLEARRA_PROJECTION_INITIAL_BUCKET_COUNT ||
        (bucket_count & (bucket_count - 1u)) != 0u ||
        bucket_count >
            (SIZE_MAX - sizeof(ClearraGeometryProjectionBucketTable)) /
                sizeof(ClearraGeometryProjectionCacheEntry *)) {
        return false;
    }
    size_t bucket_bytes = (size_t)bucket_count *
                          sizeof(ClearraGeometryProjectionCacheEntry *);
    size_t bytes = sizeof(ClearraGeometryProjectionBucketTable) +
                   bucket_bytes;
    ClearraGeometryProjectionBucketTable *table =
        (ClearraGeometryProjectionBucketTable *)cache_allocate(cache, bytes);
    if (table == 0) {
        return false;
    }
    ClearraGeometryProjectionCacheEntry **buckets =
        (ClearraGeometryProjectionCacheEntry **)(table + 1);
    memset(buckets, 0, bucket_bytes);
    *table = (ClearraGeometryProjectionBucketTable){
        .buckets = buckets,
        .bytes = bytes,
        .bucket_count = bucket_count,
        .entry_count = cache->entry_count,
    };

    for (ClearraGeometryProjectionCacheChunk *chunk = cache->chunks;
         chunk != 0;
         chunk = chunk->next) {
        for (uint32_t index = 0u; index < chunk->count; ++index) {
            ClearraGeometryProjectionCacheEntry *entry =
                &chunk->entries[index];
            uint32_t bucket = cache_bucket(
                entry->piece_count_key, bucket_count);
            entry->next_bucket = buckets[bucket];
            buckets[bucket] = entry;
        }
    }

    ClearraGeometryProjectionBucketTable *old_table = cache->bucket_table;
    cache->bucket_table = table;
    if (old_table != 0) {
        cache->resident_bytes -= old_table->bytes;
        free(old_table);
    }
    return true;
}

static bool cache_prepare_insert(
    ClearraGeometryProjectionReachabilityCache *cache) {
    if (cache->bucket_table == 0 &&
        !cache_replace_bucket_table(
            cache, CLEARRA_PROJECTION_INITIAL_BUCKET_COUNT)) {
        return false;
    }
    ClearraGeometryProjectionBucketTable *table = cache->bucket_table;
    if ((uint64_t)(cache->entry_count + 1u) * 4u >=
        (uint64_t)table->bucket_count * 3u) {
        if (table->bucket_count > UINT32_MAX / 2u ||
            !cache_replace_bucket_table(cache, table->bucket_count * 2u)) {
            return false;
        }
    }
    if (cache->tail == 0 ||
        cache->tail->count == CLEARRA_PROJECTION_CACHE_CHUNK_CAPACITY) {
        ClearraGeometryProjectionCacheChunk *chunk =
            (ClearraGeometryProjectionCacheChunk *)cache_allocate(
                cache, sizeof(*chunk));
        if (chunk == 0) {
            return false;
        }
        chunk->next = 0;
        chunk->count = 0u;
        if (cache->tail == 0) {
            cache->chunks = chunk;
        } else {
            cache->tail->next = chunk;
        }
        cache->tail = chunk;
    }
    return true;
}

static void cache_store_owned(
    ClearraGeometryProjectionReachabilityCache *cache,
    uint64_t piece_count_key,
    ClearraGeometryColumnSignature *signatures,
    uint32_t signature_count,
    uint64_t signature_digest) {
    if (cache_find(cache, piece_count_key) != 0 ||
        signature_count > SIZE_MAX / sizeof(*signatures)) {
        free(signatures);
        return;
    }
    size_t signature_bytes =
        (size_t)signature_count * sizeof(*signatures);
    if (!cache_can_add(cache, signature_bytes)) {
        free(signatures);
        return;
    }
    cache->resident_bytes += signature_bytes;
    if (!cache_prepare_insert(cache)) {
        cache->resident_bytes -= signature_bytes;
        free(signatures);
        return;
    }
    ClearraGeometryProjectionCacheEntry *entry =
        &cache->tail->entries[cache->tail->count++];
    *entry = (ClearraGeometryProjectionCacheEntry){
        .signatures = signatures,
        .signature_digest = signature_digest,
        .piece_count_key = piece_count_key,
        .signature_count = signature_count,
    };
    ClearraGeometryProjectionBucketTable *table = cache->bucket_table;
    uint32_t bucket = cache_bucket(piece_count_key, table->bucket_count);
    entry->next_bucket = table->buckets[bucket];
    table->buckets[bucket] = entry;
    table->entry_count++;
    cache->entry_count++;
}

static size_t projection_set_occupancy_word_count(uint32_t capacity) {
    return capacity / 64u + (capacity % 64u != 0u ? 1u : 0u);
}

static bool projection_set_slot_occupied(
    const ClearraProjectionSetSegment *segment,
    uint32_t slot) {
    return (segment->occupied_words[slot / 64u] &
            (UINT64_C(1) << (slot % 64u))) != 0u;
}

static void projection_set_mark_slot_occupied(
    ClearraProjectionSetSegment *segment,
    uint32_t slot) {
    segment->occupied_words[slot / 64u] |=
        UINT64_C(1) << (slot % 64u);
}

static bool projection_set_add_segment(
    ClearraProjectionSet *set,
    uint32_t capacity,
    size_t *working_bytes,
    size_t *peak_working_bytes,
    size_t max_working_bytes) {
    if (capacity < CLEARRA_PROJECTION_SET_INITIAL_CAPACITY ||
        (capacity & (capacity - 1u)) != 0u ||
        capacity > SIZE_MAX / sizeof(ClearraGeometryColumnSignature)) {
        return false;
    }
    size_t signature_bytes =
        (size_t)capacity * sizeof(ClearraGeometryColumnSignature);
    size_t occupied_word_count =
        projection_set_occupancy_word_count(capacity);
    if (occupied_word_count > SIZE_MAX / sizeof(uint64_t)) {
        return false;
    }
    size_t occupied_bytes = occupied_word_count * sizeof(uint64_t);
    if (signature_bytes > SIZE_MAX - occupied_bytes ||
        sizeof(ClearraProjectionSetSegment) >
            SIZE_MAX - signature_bytes - occupied_bytes) {
        return false;
    }
    size_t bytes = sizeof(ClearraProjectionSetSegment) + signature_bytes +
                   occupied_bytes;
    if (bytes > SIZE_MAX - *working_bytes ||
        (max_working_bytes != SIZE_MAX &&
         *working_bytes + bytes > max_working_bytes)) {
        return false;
    }
    ClearraProjectionSetSegment *segment =
        (ClearraProjectionSetSegment *)malloc(bytes);
    if (segment == 0) {
        return false;
    }
    ClearraGeometryColumnSignature *signatures =
        (ClearraGeometryColumnSignature *)(segment + 1);
    uint64_t *occupied_words = (uint64_t *)(
        (unsigned char *)signatures + signature_bytes);
    memset(occupied_words, 0, occupied_bytes);
    *segment = (ClearraProjectionSetSegment){
        .next = set->segments,
        .signatures = signatures,
        .occupied_words = occupied_words,
        .bytes = bytes,
        .capacity = capacity,
    };
    set->segments = segment;
    set->active = segment;
    set->bytes += bytes;
    *working_bytes += bytes;
    if (*working_bytes > *peak_working_bytes) {
        *peak_working_bytes = *working_bytes;
    }
    return true;
}

static void projection_set_release(
    ClearraProjectionSet *set,
    size_t *working_bytes) {
    ClearraProjectionSetSegment *segment = set->segments;
    while (segment != 0) {
        ClearraProjectionSetSegment *next = segment->next;
        free(segment);
        segment = next;
    }
    *working_bytes = set->bytes > *working_bytes
        ? 0u
        : *working_bytes - set->bytes;
    *set = (ClearraProjectionSet){0};
}

static void projection_set_reset(ClearraProjectionSet *set) {
    set->count = 0u;
    for (ClearraProjectionSetSegment *segment = set->segments;
         segment != 0;
         segment = segment->next) {
        memset(
            segment->occupied_words,
            0,
            projection_set_occupancy_word_count(segment->capacity) *
                sizeof(uint64_t));
        segment->count = 0u;
    }
}

static bool projection_set_grow(
    ClearraProjectionSet *set,
    size_t *working_bytes,
    size_t *peak_working_bytes,
    size_t max_working_bytes) {
    if (set->active == 0 || set->active->capacity > UINT32_MAX / 2u) {
        return false;
    }
    return projection_set_add_segment(
        set,
        set->active->capacity * 2u,
        working_bytes,
        peak_working_bytes,
        max_working_bytes);
}

static bool projection_set_contains(
    const ClearraProjectionSet *set,
    ClearraGeometryColumnSignature signature) {
    uint64_t hash = signature_hash(signature);
    for (const ClearraProjectionSetSegment *segment = set->segments;
         segment != 0;
         segment = segment->next) {
        uint32_t index = (uint32_t)(hash & (segment->capacity - 1u));
        for (uint32_t probe = 0u; probe < segment->capacity; ++probe) {
            if (!projection_set_slot_occupied(segment, index)) {
                break;
            }
            if (signature_equal(segment->signatures[index], signature)) {
                return true;
            }
            index = (index + 1u) & (segment->capacity - 1u);
        }
    }
    return false;
}

static ClearraProjectionSetSegment *projection_set_available_segment(
    ClearraProjectionSet *set) {
    for (ClearraProjectionSetSegment *segment = set->segments;
         segment != 0;
         segment = segment->next) {
        if ((uint64_t)(segment->count + 1u) * 4u <
            (uint64_t)segment->capacity * 3u) {
            return segment;
        }
    }
    return 0;
}

static bool projection_set_insert(
    ClearraProjectionSet *set,
    ClearraGeometryColumnSignature signature,
    size_t *working_bytes,
    size_t *peak_working_bytes,
    size_t max_working_bytes) {
    if (projection_set_contains(set, signature)) {
        return true;
    }
    ClearraProjectionSetSegment *segment =
        projection_set_available_segment(set);
    if (set->active == 0) {
        if (!projection_set_add_segment(
                set,
                CLEARRA_PROJECTION_SET_INITIAL_CAPACITY,
                working_bytes,
                peak_working_bytes,
                max_working_bytes)) {
            return false;
        }
        segment = set->active;
    } else if (segment == 0) {
        if (!projection_set_grow(
                set,
                working_bytes,
                peak_working_bytes,
                max_working_bytes)) {
            return false;
        }
        segment = set->active;
    }
    uint32_t index = (uint32_t)(
        signature_hash(signature) & (segment->capacity - 1u));
    while (projection_set_slot_occupied(segment, index)) {
        index = (index + 1u) & (segment->capacity - 1u);
    }
    segment->signatures[index] = signature;
    projection_set_mark_slot_occupied(segment, index);
    segment->count++;
    set->count++;
    return true;
}

static uint8_t signature_column(
    ClearraGeometryColumnSignature signature,
    uint8_t column) {
    return column < 12u
        ? (uint8_t)((signature.low >> (column * 5u)) & UINT64_C(31))
        : (uint8_t)((signature.high >> ((column - 12u) * 5u)) & UINT32_C(31));
}

static void signature_set_column(
    ClearraGeometryColumnSignature *signature,
    uint8_t column,
    uint8_t count) {
    if (column < 12u) {
        signature->low |= (uint64_t)count << (column * 5u);
    } else {
        signature->high |=
            (uint32_t)count << ((column - 12u) * 5u);
    }
}

static bool signature_add_bounded(
    ClearraGeometryColumnSignature left,
    ClearraGeometryColumnSignature right,
    uint8_t width,
    uint8_t maximum_column_count,
    ClearraGeometryColumnSignature *out_sum) {
    ClearraGeometryColumnSignature sum = {0u, 0u};
    for (uint8_t column = 0u; column < width; ++column) {
        uint8_t value = (uint8_t)(
            signature_column(left, column) +
            signature_column(right, column));
        if (value > maximum_column_count) {
            return false;
        }
        signature_set_column(&sum, column, value);
    }
    *out_sum = sum;
    return true;
}

static ClearraGeometryColumnSignature demand_signature(
    ClearraBoard64Layout layout,
    uint64_t remaining_cells) {
    uint8_t counts[16] = {0u};
    while (remaining_cells != 0u) {
        uint64_t bit = remaining_cells & (~remaining_cells + UINT64_C(1));
        uint8_t cell = 0u;
        for (uint64_t cursor = bit;
             (cursor & UINT64_C(1)) == 0u;
             cursor >>= 1u) {
            cell++;
        }
        counts[cell % layout.width]++;
        remaining_cells &= ~bit;
    }
    ClearraGeometryColumnSignature signature = {0u, 0u};
    for (uint8_t column = 0u; column < layout.width; ++column) {
        signature_set_column(&signature, column, counts[column]);
    }
    return signature;
}

static bool cache_entry_contains(
    const ClearraGeometryProjectionCacheEntry *entry,
    ClearraGeometryColumnSignature signature) {
    uint32_t low = 0u;
    uint32_t high = entry->signature_count;
    while (low < high) {
        uint32_t middle = low + (high - low) / 2u;
        int comparison = compare_signatures(
            &entry->signatures[middle], &signature);
        if (comparison < 0) {
            low = middle + 1u;
        } else {
            high = middle;
        }
    }
    return low < entry->signature_count &&
           signature_equal(entry->signatures[low], signature);
}

static uint64_t pack_remaining_counts(
    const uint8_t counts[CLR_STANDARD_PIECE_KIND_COUNT]) {
    uint64_t packed = 0u;
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        packed |= (uint64_t)counts[piece]
                  << ((uint32_t)(piece - CLR_PIECE_I) * 8u);
    }
    return packed;
}

static bool build_reachable_signatures(
    ClearraGeometryExactCoverSearch *search,
    const uint8_t remaining_counts[CLR_STANDARD_PIECE_KIND_COUNT],
    ClearraGeometryColumnSignature demand,
    bool *out_contains,
    uint32_t *out_signature_count,
    uint64_t *out_signature_digest) {
    size_t max_working_bytes = search->projection_cache.max_resident_bytes;
    if (max_working_bytes != SIZE_MAX) {
        max_working_bytes = max_working_bytes >
                search->projection_cache.resident_bytes
            ? max_working_bytes - search->projection_cache.resident_bytes
            : 0u;
    }
    size_t working_bytes = 0u;
    size_t peak_working_bytes = 0u;
    ClearraProjectionSet current = {0};
    ClearraProjectionSet next = {0};
    if (!projection_set_add_segment(
            &current,
            CLEARRA_PROJECTION_SET_INITIAL_CAPACITY,
            &working_bytes,
            &peak_working_bytes,
            max_working_bytes) ||
        !projection_set_add_segment(
            &next,
            CLEARRA_PROJECTION_SET_INITIAL_CAPACITY,
            &working_bytes,
            &peak_working_bytes,
            max_working_bytes) ||
        !projection_set_insert(
            &current,
            (ClearraGeometryColumnSignature){0u, 0u},
            &working_bytes,
            &peak_working_bytes,
            max_working_bytes)) {
        projection_set_release(&current, &working_bytes);
        projection_set_release(&next, &working_bytes);
        return false;
    }

    bool complete = true;
    for (uint8_t piece = CLR_PIECE_I;
         piece <= CLR_PIECE_L && complete;
         ++piece) {
        uint32_t projection_begin =
            search->catalog->piece_projection_offsets[piece];
        uint32_t projection_end =
            search->catalog->piece_projection_offsets[piece + 1u];
        for (uint8_t copy = 0u;
             copy < remaining_counts[piece] && complete;
             ++copy) {
            projection_set_reset(&next);
            for (ClearraProjectionSetSegment *segment = current.segments;
                 segment != 0 && complete;
                 segment = segment->next) {
                for (uint32_t slot = 0u;
                     slot < segment->capacity && complete;
                     ++slot) {
                    if (!projection_set_slot_occupied(segment, slot)) {
                        continue;
                    }
                    for (uint32_t projection = projection_begin;
                         projection < projection_end;
                         ++projection) {
                        ClearraGeometryColumnSignature sum;
                        if (!signature_add_bounded(
                                segment->signatures[slot],
                                search->catalog
                                    ->piece_column_projections[projection]
                                    .signature,
                                search->catalog->layout.width,
                                search->catalog->layout.height,
                                &sum)) {
                            continue;
                        }
                        if (!projection_set_insert(
                                &next,
                                sum,
                                &working_bytes,
                                &peak_working_bytes,
                                max_working_bytes)) {
                            complete = false;
                            break;
                        }
                    }
                }
            }
            ClearraProjectionSet swap = current;
            current = next;
            next = swap;
        }
    }
    if (!complete) {
        projection_set_release(&current, &working_bytes);
        projection_set_release(&next, &working_bytes);
        return false;
    }

    *out_contains = false;
    uint64_t signature_xor = 0u;
    uint64_t signature_sum = 0u;
    ClearraGeometryColumnSignature *compact = 0;
    size_t compact_bytes = 0u;
    if (current.count <= SIZE_MAX / sizeof(*compact)) {
        compact_bytes = (size_t)current.count * sizeof(*compact);
        bool compact_fits = compact_bytes <= SIZE_MAX - working_bytes &&
                            (max_working_bytes == SIZE_MAX ||
                             working_bytes + compact_bytes <=
                                 max_working_bytes);
        if (compact_fits && compact_bytes != 0u) {
            compact = (ClearraGeometryColumnSignature *)malloc(compact_bytes);
            if (compact != 0) {
                working_bytes += compact_bytes;
                if (working_bytes > peak_working_bytes) {
                    peak_working_bytes = working_bytes;
                }
            }
        }
    }
    uint32_t cursor = 0u;
    for (ClearraProjectionSetSegment *segment = current.segments;
         segment != 0;
         segment = segment->next) {
        for (uint32_t slot = 0u; slot < segment->capacity; ++slot) {
            if (!projection_set_slot_occupied(segment, slot)) {
                continue;
            }
            ClearraGeometryColumnSignature signature =
                segment->signatures[slot];
            if (compact != 0) {
                compact[cursor] = signature;
            }
            cursor++;
            if (signature_equal(signature, demand)) {
                *out_contains = true;
            }
            uint64_t term = signature_hash(signature);
            signature_xor ^= term;
            signature_sum += term;
        }
    }
    uint64_t digest = clearra_cache_key_mix_u64(
        clearra_cache_key_mix_u64(
            UINT64_C(1469598103934665603), signature_xor),
        signature_sum ^ cursor);
    if (compact != 0 && cursor > 1u) {
        qsort(compact, cursor, sizeof(*compact), compare_signatures);
    }
    uint64_t key = pack_remaining_counts(remaining_counts);
    clr_resource_report_observe_cpu_bytes(
        search->resource_report,
        clearra_geometry_search_resident_bytes(search) >
                SIZE_MAX - peak_working_bytes
            ? SIZE_MAX
            : clearra_geometry_search_resident_bytes(search) +
                  peak_working_bytes);
    projection_set_release(&current, &working_bytes);
    projection_set_release(&next, &working_bytes);
    if (compact != 0 || cursor == 0u) {
        working_bytes = compact_bytes > working_bytes
            ? 0u
            : working_bytes - compact_bytes;
        cache_store_owned(
            &search->projection_cache, key, compact, cursor, digest);
    }
    clr_resource_report_observe_cpu_bytes(
        search->resource_report,
        clearra_geometry_search_resident_bytes(search));
    *out_signature_count = cursor;
    *out_signature_digest = digest;
    return true;
}

static bool active_family_contains(
    const ClearraActivePieceFamily *active_family,
    uint16_t member_index) {
    return (active_family->words[member_index / 64u] &
            (UINT64_C(1) << (member_index % 64u))) != 0u;
}

static bool remaining_counts_for_member(
    const ClearraGeometryExactCoverSearch *search,
    uint16_t member_index,
    uint8_t remaining_piece_count,
    uint8_t out_counts[CLR_STANDARD_PIECE_KIND_COUNT]) {
    const clr_piece_multiset_window *member =
        search->problem->piece_multiset_family.count == 0u
        ? &search->problem->piece_multiset_window
        : &search->problem->piece_multiset_family.members[member_index];
    memset(out_counts, 0, CLR_STANDARD_PIECE_KIND_COUNT);
    uint8_t total = 0u;
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        if (member->counts[piece] < search->used_piece_counts[piece]) {
            return false;
        }
        out_counts[piece] = (uint8_t)(
            member->counts[piece] - search->used_piece_counts[piece]);
        total = (uint8_t)(total + out_counts[piece]);
    }
    return total == remaining_piece_count;
}

ClearraGeometryProjectionReachabilityStatus
clearra_geometry_projection_reachability_propagate(
    ClearraGeometryExactCoverSearch *search,
    const ClearraActivePieceFamily *active_family,
    uint64_t remaining_cells,
    uint8_t remaining_piece_count,
    ClearraGeometryProjectionReachabilityResult *out_result) {
    if (search == 0 || search->catalog == 0 || active_family == 0 ||
        out_result == 0 || remaining_cells == 0u ||
        remaining_piece_count == 0u || search->catalog->layout.width > 16u ||
        search->catalog->piece_column_projections == 0) {
        return CLEARRA_GEOMETRY_PROJECTION_INVALID;
    }
    ClearraGeometryColumnSignature demand = demand_signature(
        search->catalog->layout, remaining_cells);
    ClearraGeometryProjectionReachabilityResult result = {
        .evidence_digest = clearra_cache_key_mix_u64(
            clearra_cache_key_mix_u64(
                UINT64_C(1469598103934665603),
                CLEARRA_PROJECTION_REACHABILITY_PROOF_VERSION),
            demand.low),
    };
    result.evidence_digest = clearra_cache_key_mix_u64(
        result.evidence_digest, demand.high);

    uint64_t checked_keys[CLR_PIECE_MULTISET_FAMILY_CAPACITY];
    uint16_t checked_key_count = 0u;
    bool skipped = false;
    uint16_t begin = search->problem->piece_multiset_family.count == 0u
        ? 0u
        : search->family_begin;
    uint16_t end = search->problem->piece_multiset_family.count == 0u
        ? 1u
        : search->family_end;
    for (uint16_t member = begin; member < end; ++member) {
        if (search->problem->piece_multiset_family.count != 0u &&
            !active_family_contains(active_family, member)) {
            continue;
        }
        uint8_t remaining_counts[CLR_STANDARD_PIECE_KIND_COUNT];
        if (!remaining_counts_for_member(
                search,
                member,
                remaining_piece_count,
                remaining_counts)) {
            continue;
        }
        uint64_t key = pack_remaining_counts(remaining_counts);
        bool duplicate = false;
        for (uint16_t index = 0u; index < checked_key_count; ++index) {
            if (checked_keys[index] == key) {
                duplicate = true;
                break;
            }
        }
        if (duplicate) {
            continue;
        }
        checked_keys[checked_key_count++] = key;
        result.checked_piece_count_vectors++;
        result.evidence_digest = clearra_cache_key_mix_u64(
            result.evidence_digest, key);

        ClearraGeometryProjectionCacheEntry *entry = cache_find(
            &search->projection_cache, key);
        bool contains = false;
        uint32_t signature_count = 0u;
        uint64_t signature_digest = 0u;
        if (entry != 0) {
            result.cache_hits++;
            contains = cache_entry_contains(entry, demand);
            signature_count = entry->signature_count;
            signature_digest = entry->signature_digest;
        } else {
            result.cache_misses++;
            if (!build_reachable_signatures(
                    search,
                    remaining_counts,
                    demand,
                    &contains,
                    &signature_count,
                    &signature_digest)) {
                skipped = true;
                continue;
            }
        }
        result.reachable_signature_count += signature_count;
        result.evidence_digest = clearra_cache_key_mix_u64(
            result.evidence_digest, signature_digest);
        if (contains) {
            *out_result = result;
            return CLEARRA_GEOMETRY_PROJECTION_REACHABLE;
        }
    }
    if (result.checked_piece_count_vectors == 0u) {
        return CLEARRA_GEOMETRY_PROJECTION_INVALID;
    }
    if (result.evidence_digest == 0u) {
        result.evidence_digest = UINT64_C(1);
    }
    *out_result = result;
    return skipped
        ? CLEARRA_GEOMETRY_PROJECTION_SKIPPED
        : CLEARRA_GEOMETRY_PROJECTION_UNREACHABLE;
}
