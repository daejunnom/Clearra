#ifndef CLEARRA_GEOMETRY_SOLUTION_GRAPH_INTERNAL_H
#define CLEARRA_GEOMETRY_SOLUTION_GRAPH_INTERNAL_H

#include "geometry_solution_family.h"
#include "packing_problem.h"

struct ClearraGeometrySolutionGraph {
    ClearraGeometrySolutionFamily family;
    ClearraGeometryCatalogIdentity catalog_identity;
    ClearraGeometryFamilyRef root;
    size_t resident_bytes;
    uint32_t skeleton_count;
    uint8_t target_depth;
    uint8_t complete;
    uint8_t reserved[2];
};

#endif
