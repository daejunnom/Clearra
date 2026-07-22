#ifndef CLEARRA_GEOMETRY_PROJECTION_REACHABILITY_H
#define CLEARRA_GEOMETRY_PROJECTION_REACHABILITY_H

#include <stddef.h>
#include <stdint.h>

typedef struct ClearraActivePieceFamily ClearraActivePieceFamily;
typedef struct ClearraGeometryExactCoverSearch ClearraGeometryExactCoverSearch;
typedef struct ClearraGeometryProjectionCacheEntry
    ClearraGeometryProjectionCacheEntry;
typedef struct ClearraGeometryProjectionCacheChunk
    ClearraGeometryProjectionCacheChunk;
typedef struct ClearraGeometryProjectionBucketTable
    ClearraGeometryProjectionBucketTable;

typedef struct ClearraGeometryColumnSignature {
    uint64_t low;
    uint32_t high;
} ClearraGeometryColumnSignature;

typedef struct ClearraGeometryProjectionReachabilityCache {
    ClearraGeometryProjectionCacheChunk *chunks;
    ClearraGeometryProjectionCacheChunk *tail;
    ClearraGeometryProjectionBucketTable *bucket_table;
    size_t resident_bytes;
    size_t max_resident_bytes;
    uint32_t entry_count;
} ClearraGeometryProjectionReachabilityCache;

typedef enum ClearraGeometryProjectionReachabilityStatus {
    CLEARRA_GEOMETRY_PROJECTION_REACHABLE = 0,
    CLEARRA_GEOMETRY_PROJECTION_UNREACHABLE = 1,
    CLEARRA_GEOMETRY_PROJECTION_SKIPPED = 2,
    CLEARRA_GEOMETRY_PROJECTION_INVALID = 3
} ClearraGeometryProjectionReachabilityStatus;

typedef struct ClearraGeometryProjectionReachabilityResult {
    uint64_t evidence_digest;
    uint64_t reachable_signature_count;
    uint32_t checked_piece_count_vectors;
    uint32_t cache_hits;
    uint32_t cache_misses;
} ClearraGeometryProjectionReachabilityResult;

void clearra_geometry_projection_cache_init(
    ClearraGeometryProjectionReachabilityCache *cache,
    size_t max_resident_bytes);

void clearra_geometry_projection_cache_release(
    ClearraGeometryProjectionReachabilityCache *cache);

ClearraGeometryProjectionReachabilityStatus
clearra_geometry_projection_reachability_propagate(
    ClearraGeometryExactCoverSearch *search,
    const ClearraActivePieceFamily *active_family,
    uint64_t remaining_cells,
    uint8_t remaining_piece_count,
    ClearraGeometryProjectionReachabilityResult *out_result);

#endif
