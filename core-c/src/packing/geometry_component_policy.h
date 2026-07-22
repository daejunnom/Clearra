#ifndef CLEARRA_GEOMETRY_COMPONENT_POLICY_H
#define CLEARRA_GEOMETRY_COMPONENT_POLICY_H

#include "geometry_catalog_internal.h"

#include <stdbool.h>
#include <stdint.h>

bool clearra_geometry_component_analysis_should_run(
    const ClearraGeometryCatalog *catalog,
    uint64_t remaining_cells,
    uint8_t depth);

#endif
