#ifndef CLEARRA_GEOMETRY_EXACT_COVER_INTERNAL_H
#define CLEARRA_GEOMETRY_EXACT_COVER_INTERNAL_H

#include "geometry_catalog_internal.h"
#include "geometry_piece_family_domain.h"
#include "geometry_residual_memo.h"
#include "geometry_solution_family.h"

typedef struct ClearraGeometryExactCoverOutput {
    ClearraPackingCandidateBuffer *buffer;
    const ClearraPackingCandidateSink *sink;
    size_t accepted_count;
    size_t host_resident_bytes;
    size_t max_candidate_rows;
    size_t max_total_bytes;
} ClearraGeometryExactCoverOutput;

typedef struct ClearraGeometryExactCoverSearch {
    const ClearraGeometryCatalog *catalog;
    const clr_packing_problem *problem;
    clr_resource_report *resource_report;
    ClearraGeometryExactCoverOutput output;
    uint32_t selected_rows[CLEARRA_PACKING_MAX_PIECES];
    uint8_t used_piece_counts[CLR_STANDARD_PIECE_KIND_COUNT];
    ClearraGeometryPieceFamilyDomain piece_family_domain;
    uint8_t target_depth;
    uint16_t family_begin;
    uint16_t family_end;
    uint16_t partition_index;
    uint16_t partition_count;
    uint8_t partition_depth;
    uint64_t expanded_nodes;
    size_t component_workspace_bytes;
    uint64_t component_decomposition_count;
    uint32_t cancellation_poll_counter;
    clr_pruning_proof_ledger *pruning_ledger;
    uint64_t pruning_batch_id;
    uint64_t pruning_catalog_identity_digest;
    ClearraGeometryResidualMemo residual_memo;
    ClearraGeometrySolutionFamily solution_family;
    ClearraGeometryProjectionReachabilityCache projection_cache;
} ClearraGeometryExactCoverSearch;

size_t clearra_geometry_search_resident_bytes(
    const ClearraGeometryExactCoverSearch *search);

bool clearra_geometry_row_is_feasible(
    const ClearraGeometryExactCoverSearch *search,
    const ClearraActivePieceFamily *active_family,
    uint32_t row_id,
    uint64_t remaining_cells,
    ClearraActivePieceFamily *out_next_family);

ClearraPackingStatus clearra_geometry_search_exact_cover(
    ClearraGeometryExactCoverSearch *search,
    uint64_t remaining_cells,
    uint8_t depth,
    uint64_t prefix_hash,
    const ClearraActivePieceFamily *active_family,
    ClearraGeometryFamilyRef *out_family);

ClearraPackingStatus clearra_geometry_emit_solution_family(
    ClearraGeometryExactCoverSearch *search,
    ClearraGeometryFamilyRef family,
    uint8_t depth);

ClearraPackingStatus clearra_geometry_try_component_composition(
    ClearraGeometryExactCoverSearch *search,
    uint64_t remaining_cells,
    uint8_t depth,
    uint64_t prefix_hash,
    const ClearraActivePieceFamily *active_family,
    bool *out_applied,
    ClearraGeometryFamilyRef *out_family);

#endif
