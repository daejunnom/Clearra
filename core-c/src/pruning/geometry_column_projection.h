#ifndef CLEARRA_GEOMETRY_COLUMN_PROJECTION_H
#define CLEARRA_GEOMETRY_COLUMN_PROJECTION_H

#include "../packing/geometry_exact_cover_internal.h"

typedef enum ClearraGeometryColumnProjectionStatus {
    CLEARRA_GEOMETRY_COLUMN_PROJECTION_SUPPORTED = 0,
    CLEARRA_GEOMETRY_COLUMN_PROJECTION_IMPOSSIBLE = 1,
    CLEARRA_GEOMETRY_COLUMN_PROJECTION_SKIPPED = 2,
    CLEARRA_GEOMETRY_COLUMN_PROJECTION_INVALID = 3
} ClearraGeometryColumnProjectionStatus;

typedef struct ClearraGeometryColumnProjectionResult {
    uint64_t evidence_digest;
    uint32_t feasible_row_count;
    uint8_t failed_column;
    uint8_t demand;
    uint8_t relaxed_minimum;
    uint8_t relaxed_maximum;
} ClearraGeometryColumnProjectionResult;

ClearraGeometryColumnProjectionStatus
clearra_geometry_column_projection_propagate(
    const ClearraGeometryExactCoverSearch *search,
    const ClearraActivePieceFamily *active_family,
    uint64_t remaining_cells,
    uint8_t remaining_piece_count,
    ClearraGeometryColumnProjectionResult *out_result);

#endif
