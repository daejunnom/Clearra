#ifndef CLEARRA_GEOMETRY_APDP_H
#define CLEARRA_GEOMETRY_APDP_H

#include "../packing/geometry_exact_cover_internal.h"

#define CLEARRA_APDP_SUPPORT_ARM_ARM UINT8_C(1)
#define CLEARRA_APDP_SUPPORT_ARM_ELBOW UINT8_C(2)
#define CLEARRA_APDP_SUPPORT_ELBOW_ELBOW UINT8_C(4)

typedef enum ClearraGeometryApdpStatus {
    CLEARRA_GEOMETRY_APDP_SUPPORTED = 0,
    CLEARRA_GEOMETRY_APDP_EMPTY = 1,
    CLEARRA_GEOMETRY_APDP_SKIPPED = 2,
    CLEARRA_GEOMETRY_APDP_INVALID = 3
} ClearraGeometryApdpStatus;

typedef struct ClearraGeometryApdpResult {
    uint64_t exact_parent_row_digest;
    uint64_t parent_piece_mask;
    uint32_t exact_parent_row_count;
    uint32_t filtered_parent_row_count;
    uint8_t partial_shape_kind;
    uint8_t reserved[7];
} ClearraGeometryApdpResult;

bool clearra_geometry_apdp_compile_support_flags(
    ClearraBoard64Layout layout,
    const uint64_t *skeleton_cell_masks,
    uint32_t skeleton_count,
    uint8_t *out_support_flags);

bool clearra_geometry_apdp_row_supports_required_cells(
    const ClearraGeometryCatalog *catalog,
    uint32_t row_id,
    uint64_t required_same_tile_cells);

ClearraGeometryApdpStatus clearra_geometry_apdp_propagate(
    const ClearraGeometryExactCoverSearch *search,
    const ClearraActivePieceFamily *active_family,
    uint64_t remaining_cells,
    uint64_t required_same_tile_cells,
    ClearraGeometryApdpResult *out_result);

#endif
