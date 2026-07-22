#ifndef CLEARRA_BUILDUP_OPERATION_VARIANT_CACHE_H
#define CLEARRA_BUILDUP_OPERATION_VARIANT_CACHE_H

#include "buildup_internal.h"

typedef struct ClearraBuildUpOperationVariantCacheKey {
    uint64_t geometry_mask;
    uint16_t deleted_row_mask;
    uint8_t width;
    uint8_t height;
    uint8_t piece;
    uint8_t count;
    uint16_t reserved;
} ClearraBuildUpOperationVariantCacheKey;

typedef struct ClearraBuildUpOperationVariantCacheValue {
    clr_buildup_operation variants[CLR_BUILDUP_MAX_OPERATION_VARIANTS];
} ClearraBuildUpOperationVariantCacheValue;

typedef struct ClearraBuildUpOperationVariantCache {
    ClearraBuildUpOperationVariantCacheKey *keys;
    ClearraBuildUpOperationVariantCacheValue *values;
    uint8_t *occupied;
    uint64_t catalog_identity_digest;
    uint32_t capacity;
    uint64_t insertion_count;
    uint64_t collision_count;
} ClearraBuildUpOperationVariantCache;

void clearra_buildup_operation_variant_cache_prepare(
    ClearraBuildUpOperationVariantCache *cache,
    const clr_buildup_problem *problem);
void clearra_buildup_operation_variant_cache_release(
    ClearraBuildUpOperationVariantCache *cache);
bool clearra_buildup_operation_variant_cache_lookup(
    const ClearraBuildUpOperationVariantCache *cache,
    ClearraBoard64Layout layout,
    uint8_t piece,
    uint64_t geometry_mask,
    uint16_t deleted_row_mask,
    clr_buildup_operation out_variants[CLR_BUILDUP_MAX_OPERATION_VARIANTS],
    uint8_t *out_count);
void clearra_buildup_operation_variant_cache_insert(
    ClearraBuildUpOperationVariantCache *cache,
    ClearraBoard64Layout layout,
    uint8_t piece,
    uint64_t geometry_mask,
    uint16_t deleted_row_mask,
    const clr_buildup_operation variants[CLR_BUILDUP_MAX_OPERATION_VARIANTS],
    uint8_t count);

#endif
