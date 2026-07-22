#ifndef CLEARRA_GEOMETRY_COMPONENT_DECOMPOSITION_H
#define CLEARRA_GEOMETRY_COMPONENT_DECOMPOSITION_H

#include "geometry_catalog_internal.h"

#include <stdbool.h>
#include <stdint.h>

#define CLEARRA_GEOMETRY_MAX_COMPONENTS 64u

typedef bool (*ClearraGeometryRowPredicate)(void *context, uint32_t row_id);

typedef struct ClearraGeometryComponentDecomposition {
    uint64_t component_masks[CLEARRA_GEOMETRY_MAX_COMPONENTS];
    uint64_t unsupported_cells;
    uint32_t feasible_row_count;
    uint8_t component_count;
} ClearraGeometryComponentDecomposition;

typedef struct ClearraGeometryComponentCompositionPlan {
    uint64_t owner_component_mask;
    uint64_t remainder_mask;
    uint8_t component_count;
} ClearraGeometryComponentCompositionPlan;

bool clearra_geometry_component_decompose(
    const ClearraGeometryCatalog *catalog,
    uint64_t remaining_cells,
    ClearraGeometryRowPredicate row_is_feasible,
    void *predicate_context,
    ClearraGeometryComponentDecomposition *out_decomposition);

bool clearra_geometry_component_make_composition_plan(
    const ClearraGeometryComponentDecomposition *decomposition,
    uint64_t remaining_cells,
    ClearraGeometryComponentCompositionPlan *out_plan);

#endif
