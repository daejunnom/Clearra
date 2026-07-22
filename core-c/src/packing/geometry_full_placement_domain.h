#ifndef CLEARRA_GEOMETRY_FULL_PLACEMENT_DOMAIN_H
#define CLEARRA_GEOMETRY_FULL_PLACEMENT_DOMAIN_H

#include "geometry_exact_cover_internal.h"

typedef enum ClearraGeometryDomainStatus {
    CLEARRA_GEOMETRY_DOMAIN_SUPPORTED = 0,
    CLEARRA_GEOMETRY_DOMAIN_EMPTY = 1,
    CLEARRA_GEOMETRY_DOMAIN_INVALID = 2
} ClearraGeometryDomainStatus;

typedef struct ClearraGeometryDomainPropagation {
    uint8_t cell_piece_masks[64];
    uint64_t pivot_required_cells;
    uint64_t evidence_digest;
    uint64_t pivot_piece_mask;
    uint32_t pivot_support_count;
    uint32_t pivot_filtered_row_count;
    uint8_t pivot_cell;
    uint8_t same_tile_certificate_count;
    uint8_t reserved[2];
} ClearraGeometryDomainPropagation;

ClearraGeometryDomainStatus clearra_geometry_full_placement_domain_propagate(
    const ClearraGeometryExactCoverSearch *search,
    const ClearraActivePieceFamily *active_family,
    uint64_t remaining_cells,
    ClearraGeometryDomainPropagation *out_propagation);

#endif
