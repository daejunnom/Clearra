#ifndef CLEARRA_GEOMETRY_ADDITIVE_INVARIANT_H
#define CLEARRA_GEOMETRY_ADDITIVE_INVARIANT_H

#include "../packing/geometry_exact_cover_internal.h"

#define CLEARRA_ADDITIVE_SIGNATURE_BANK_COUNT 3u
#define CLEARRA_ADDITIVE_INVARIANT_PROOF_VERSION UINT64_C(1)

typedef enum ClearraGeometryInvariantStatus {
    CLEARRA_GEOMETRY_INVARIANT_SUPPORTED = 0,
    CLEARRA_GEOMETRY_INVARIANT_IMPOSSIBLE = 1,
    CLEARRA_GEOMETRY_INVARIANT_INVALID = 2
} ClearraGeometryInvariantStatus;

typedef struct ClearraGeometryInvariantResult {
    uint64_t evidence_digest;
    uint8_t failed_bank;
    uint8_t checked_bank_count;
    uint8_t reserved[6];
} ClearraGeometryInvariantResult;

bool clearra_geometry_additive_invariant_compile_signatures(
    ClearraBoard64Layout layout,
    const uint64_t *skeleton_cell_masks,
    uint32_t skeleton_count,
    uint8_t *out_signatures);

ClearraGeometryInvariantStatus clearra_geometry_additive_invariant_propagate(
    const ClearraGeometryExactCoverSearch *search,
    const ClearraActivePieceFamily *active_family,
    uint64_t remaining_cells,
    uint8_t remaining_piece_count,
    ClearraGeometryInvariantResult *out_result);

#endif
