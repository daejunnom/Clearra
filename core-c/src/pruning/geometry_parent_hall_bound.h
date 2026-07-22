#ifndef CLEARRA_GEOMETRY_PARENT_HALL_BOUND_H
#define CLEARRA_GEOMETRY_PARENT_HALL_BOUND_H

#include "../packing/geometry_full_placement_domain.h"

typedef enum ClearraGeometryHallStatus {
    CLEARRA_GEOMETRY_HALL_SUPPORTED = 0,
    CLEARRA_GEOMETRY_HALL_IMPOSSIBLE = 1,
    CLEARRA_GEOMETRY_HALL_SKIPPED = 2,
    CLEARRA_GEOMETRY_HALL_INVALID = 3
} ClearraGeometryHallStatus;

typedef struct ClearraGeometryHallResult {
    uint64_t evidence_digest;
    uint8_t compact_piece_subset;
    uint8_t constrained_cell_count;
    uint8_t maximum_piece_count;
    uint8_t active_family_member_count;
} ClearraGeometryHallResult;

ClearraGeometryHallStatus clearra_geometry_parent_hall_bound_propagate(
    const ClearraGeometryExactCoverSearch *search,
    const ClearraActivePieceFamily *active_family,
    const ClearraGeometryDomainPropagation *domain,
    uint64_t remaining_cells,
    ClearraGeometryHallResult *out_result);

#endif
