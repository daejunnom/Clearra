#ifndef CLEARRA_PACKING_CANDIDATE_MATERIALIZER_H
#define CLEARRA_PACKING_CANDIDATE_MATERIALIZER_H

#include "packing_problem.h"

typedef struct ClearraPackingGeometryPath {
    uint32_t skeleton_ids[CLEARRA_PACKING_MAX_PIECES];
    uint8_t operation_count;
    uint8_t reserved[3];
} ClearraPackingGeometryPath;

void clearra_packing_candidate_assign_geometry_representative(
    ClearraPackingCandidateView *candidate,
    uint8_t index,
    const ClearraPlacementCandidate *representative);

void clearra_packing_candidate_finalize_geometry(
    ClearraBoard64Layout layout,
    ClearraPackingCandidateView *candidate);

ClearraPackingStatus clearra_packing_materialize_catalog_path(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    const ClearraPackingGeometryPath *path,
    ClearraPackingCandidateView *out_candidate);

ClearraPackingStatus clearra_packing_materialize_catalog_row_ids(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    const uint32_t *skeleton_row_ids,
    uint8_t operation_count,
    ClearraPackingCandidateView *out_candidate);

ClearraPackingStatus
clearra_packing_materialize_catalog_paths_to_sink(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    const ClearraPackingGeometryPath *paths,
    uint32_t path_count,
    const ClearraPackingCandidateSink *sink,
    clr_resource_report *out_resource_report);

#endif
