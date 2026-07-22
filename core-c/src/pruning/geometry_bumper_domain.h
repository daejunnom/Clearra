#ifndef CLEARRA_GEOMETRY_BUMPER_DOMAIN_H
#define CLEARRA_GEOMETRY_BUMPER_DOMAIN_H

#include "../packing/geometry_exact_cover_internal.h"

typedef enum ClearraGeometryBumperStatus {
    CLEARRA_GEOMETRY_BUMPER_SUPPORTED = 0,
    CLEARRA_GEOMETRY_BUMPER_EMPTY = 1,
    CLEARRA_GEOMETRY_BUMPER_SKIPPED = 2,
    CLEARRA_GEOMETRY_BUMPER_INVALID = 3
} ClearraGeometryBumperStatus;

typedef struct ClearraGeometryBumperResult {
    uint64_t evidence_digest;
    uint64_t parent_piece_mask;
    uint32_t exact_parent_row_count;
    uint32_t filtered_parent_row_count;
    uint8_t bumper_cell;
    uint8_t bumper_column;
    uint8_t outer_three_row_count;
    uint8_t split_two_one_row_count;
} ClearraGeometryBumperResult;

ClearraGeometryBumperStatus clearra_geometry_bumper_domain_propagate(
    const ClearraGeometryExactCoverSearch *search,
    const ClearraActivePieceFamily *active_family,
    uint64_t remaining_cells,
    ClearraGeometryBumperResult *out_result);

bool clearra_geometry_bumper_row_is_compatible(
    const ClearraGeometryCatalog *catalog,
    uint64_t remaining_cells,
    uint8_t bumper_cell,
    uint32_t row_id);

#endif
