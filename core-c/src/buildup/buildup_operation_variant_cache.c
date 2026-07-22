#include "buildup_operation_variant_cache.h"

#include "../cache/cache_identity.h"
#include "../packing/geometry_catalog_internal.h"

#include <stdlib.h>
#include <string.h>

#define CLEARRA_BUILDUP_OPERATION_VARIANT_CACHE_DEFAULT_PER_WORKER_BYTES \
    (UINT64_C(2) * UINT64_C(1024) * UINT64_C(1024))
#define CLEARRA_BUILDUP_OPERATION_VARIANT_CACHE_INITIAL_PER_WORKER_BYTES \
    (UINT64_C(256) * UINT64_C(1024))
#define CLEARRA_BUILDUP_OPERATION_VARIANT_CACHE_MIN_CAPACITY 256u

_Static_assert(
    sizeof(ClearraBuildUpOperationVariantCacheKey) == 16u,
    "operation variant cache keys must remain compact");
_Static_assert(
    sizeof(ClearraBuildUpOperationVariantCacheValue) == 64u,
    "operation variant cache values must fit one cache line");

static uint64_t cache_per_worker_budget(const clr_buildup_problem *problem) {
    if (problem->packing.budget.has_max_memory_mib == 0u) {
        return CLEARRA_BUILDUP_OPERATION_VARIANT_CACHE_DEFAULT_PER_WORKER_BYTES;
    }
    uint64_t workers = problem->packing.backend.workers == 0u
                           ? UINT64_C(1)
                           : problem->packing.backend.workers;
    uint64_t memory_bytes =
        (uint64_t)problem->packing.budget.max_memory_mib * UINT64_C(1024) *
        UINT64_C(1024);
    return memory_bytes / UINT64_C(64) / workers;
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
        sizeof(ClearraBuildUpOperationVariantCacheKey) +
        sizeof(ClearraBuildUpOperationVariantCacheValue) + sizeof(uint8_t);
    uint64_t entry_count = per_worker_bytes / bytes_per_entry;
    uint64_t allocation_limit =
        (uint64_t)(SIZE_MAX / sizeof(ClearraBuildUpOperationVariantCacheValue));
    uint64_t key_limit =
        (uint64_t)(SIZE_MAX / sizeof(ClearraBuildUpOperationVariantCacheKey));
    uint64_t occupied_limit = (uint64_t)(SIZE_MAX / sizeof(uint8_t));
    if (allocation_limit > key_limit) {
        allocation_limit = key_limit;
    }
    if (allocation_limit > occupied_limit) {
        allocation_limit = occupied_limit;
    }
    if (entry_count > allocation_limit) {
        entry_count = allocation_limit;
    }
    if (entry_count < CLEARRA_BUILDUP_OPERATION_VARIANT_CACHE_MIN_CAPACITY) {
        return 0u;
    }
    return lower_power_of_two(entry_count);
}

static uint32_t initial_capacity(uint32_t maximum) {
    const uint64_t bytes_per_entry =
        sizeof(ClearraBuildUpOperationVariantCacheKey) +
        sizeof(ClearraBuildUpOperationVariantCacheValue) + sizeof(uint8_t);
    uint32_t initial = lower_power_of_two(
        CLEARRA_BUILDUP_OPERATION_VARIANT_CACHE_INITIAL_PER_WORKER_BYTES /
        bytes_per_entry);
    if (initial < CLEARRA_BUILDUP_OPERATION_VARIANT_CACHE_MIN_CAPACITY) {
        initial = CLEARRA_BUILDUP_OPERATION_VARIANT_CACHE_MIN_CAPACITY;
    }
    return initial < maximum ? initial : maximum;
}

static uint32_t selected_capacity(
    const ClearraBuildUpOperationVariantCache *cache,
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

static uint64_t catalog_identity_digest(
    const clr_buildup_problem *problem) {
    if (problem == 0 || problem->geometry_catalog == 0) {
        return 0u;
    }
    const ClearraGeometryCatalogIdentity *identity =
        clearra_geometry_catalog_identity(problem->geometry_catalog);
    if (identity == 0) {
        return 0u;
    }
    uint64_t digest = UINT64_C(1469598103934665603);
    digest = clearra_cache_key_mix_u64(
        digest, identity->board_layout_id);
    digest = clearra_cache_key_mix_u64(
        digest, identity->compact_universe_digest);
    digest = clearra_cache_key_mix_u64(
        digest, identity->target_geometry_digest);
    digest = clearra_cache_key_mix_u64(
        digest, identity->piece_catalog_id);
    digest = clearra_cache_key_mix_u64(
        digest, identity->skeleton_projection_version);
    digest = clearra_cache_key_mix_u64(
        digest, identity->rule_capability_id);
    digest = clearra_cache_key_mix_u64(
        digest, identity->realization_table_digest);
    return digest == 0u ? UINT64_C(1) : digest;
}

void clearra_buildup_operation_variant_cache_prepare(
    ClearraBuildUpOperationVariantCache *cache,
    const clr_buildup_problem *problem) {
    if (cache == 0 || problem == 0) {
        return;
    }
    uint32_t maximum = maximum_capacity(problem);
    uint64_t identity_digest = catalog_identity_digest(problem);
    if (maximum == 0u) {
        clearra_buildup_operation_variant_cache_release(cache);
        return;
    }
    uint32_t capacity = selected_capacity(cache, maximum);
    if (cache->keys != 0 && cache->values != 0 && cache->occupied != 0 &&
        cache->capacity == capacity) {
        if (cache->catalog_identity_digest != identity_digest) {
            memset(cache->occupied, 0, (size_t)capacity);
            cache->catalog_identity_digest = identity_digest;
        }
        cache->insertion_count = 0u;
        cache->collision_count = 0u;
        return;
    }

    ClearraBuildUpOperationVariantCacheKey *keys =
        (ClearraBuildUpOperationVariantCacheKey *)malloc(
            (size_t)capacity * sizeof(*keys));
    ClearraBuildUpOperationVariantCacheValue *values =
        (ClearraBuildUpOperationVariantCacheValue *)malloc(
            (size_t)capacity * sizeof(*values));
    uint8_t *occupied = (uint8_t *)malloc((size_t)capacity);
    if (keys == 0 || values == 0 || occupied == 0) {
        free(keys);
        free(values);
        free(occupied);
        if (cache->capacity > maximum) {
            clearra_buildup_operation_variant_cache_release(cache);
        } else {
            cache->insertion_count = 0u;
            cache->collision_count = 0u;
        }
        return;
    }
    memset(occupied, 0, (size_t)capacity);
    clearra_buildup_operation_variant_cache_release(cache);
    cache->keys = keys;
    cache->values = values;
    cache->occupied = occupied;
    cache->catalog_identity_digest = identity_digest;
    cache->capacity = capacity;
    cache->insertion_count = 0u;
    cache->collision_count = 0u;
}

void clearra_buildup_operation_variant_cache_release(
    ClearraBuildUpOperationVariantCache *cache) {
    if (cache == 0) {
        return;
    }
    free(cache->keys);
    free(cache->values);
    free(cache->occupied);
    *cache = (ClearraBuildUpOperationVariantCache){0};
}

static uint32_t cache_index(
    const ClearraBuildUpOperationVariantCache *cache,
    ClearraBoard64Layout layout,
    uint8_t piece,
    uint64_t geometry_mask,
    uint16_t deleted_row_mask) {
    uint64_t hash = geometry_mask ^
                    ((uint64_t)deleted_row_mask << 32u) ^
                    ((uint64_t)piece << 48u) ^
                    ((uint64_t)layout.width << 56u) ^
                    ((uint64_t)layout.height << 60u);
    hash = (hash ^ (hash >> 30u)) * UINT64_C(0xbf58476d1ce4e5b9);
    hash = (hash ^ (hash >> 27u)) * UINT64_C(0x94d049bb133111eb);
    hash ^= hash >> 31u;
    return (uint32_t)hash & (cache->capacity - 1u);
}

bool clearra_buildup_operation_variant_cache_lookup(
    const ClearraBuildUpOperationVariantCache *cache,
    ClearraBoard64Layout layout,
    uint8_t piece,
    uint64_t geometry_mask,
    uint16_t deleted_row_mask,
    clr_buildup_operation out_variants[CLR_BUILDUP_MAX_OPERATION_VARIANTS],
    uint8_t *out_count) {
    if (cache == 0 || cache->keys == 0 || cache->values == 0 ||
        cache->occupied == 0 ||
        cache->capacity == 0u || geometry_mask == 0u || out_variants == 0 ||
        out_count == 0) {
        return false;
    }
    uint32_t index = cache_index(
        cache, layout, piece, geometry_mask, deleted_row_mask);
    if (cache->occupied[index] == 0u) {
        return false;
    }
    const ClearraBuildUpOperationVariantCacheKey *key = &cache->keys[index];
    if (key->geometry_mask != geometry_mask ||
        key->deleted_row_mask != deleted_row_mask || key->width != layout.width ||
        key->height != layout.height || key->piece != piece ||
        key->count > CLR_BUILDUP_MAX_OPERATION_VARIANTS) {
        return false;
    }
    *out_count = key->count;
    if (key->count != 0u) {
        memcpy(
            out_variants,
            cache->values[index].variants,
            (size_t)key->count * sizeof(*out_variants));
    }
    return true;
}

void clearra_buildup_operation_variant_cache_insert(
    ClearraBuildUpOperationVariantCache *cache,
    ClearraBoard64Layout layout,
    uint8_t piece,
    uint64_t geometry_mask,
    uint16_t deleted_row_mask,
    const clr_buildup_operation variants[CLR_BUILDUP_MAX_OPERATION_VARIANTS],
    uint8_t count) {
    if (cache == 0 || cache->keys == 0 || cache->values == 0 ||
        cache->occupied == 0 ||
        cache->capacity == 0u || geometry_mask == 0u || variants == 0 ||
        count > CLR_BUILDUP_MAX_OPERATION_VARIANTS) {
        return;
    }
    uint32_t index = cache_index(
        cache, layout, piece, geometry_mask, deleted_row_mask);
    cache->insertion_count++;
    if (cache->occupied[index] != 0u) {
        const ClearraBuildUpOperationVariantCacheKey *existing =
            &cache->keys[index];
        if (existing->geometry_mask != geometry_mask ||
            existing->deleted_row_mask != deleted_row_mask ||
            existing->width != layout.width ||
            existing->height != layout.height ||
            existing->piece != piece) {
            cache->collision_count++;
        }
    }
    if (count != 0u) {
        memcpy(
            cache->values[index].variants,
            variants,
            (size_t)count * sizeof(*variants));
    }
    cache->keys[index] = (ClearraBuildUpOperationVariantCacheKey){
        .geometry_mask = geometry_mask,
        .deleted_row_mask = deleted_row_mask,
        .width = layout.width,
        .height = layout.height,
        .piece = piece,
        .count = count,
    };
    cache->occupied[index] = 1u;
}
