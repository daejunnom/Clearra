#ifndef CLEARRA_GEOMETRY_EXACT_COVER_PROOF_H
#define CLEARRA_GEOMETRY_EXACT_COVER_PROOF_H

#include "geometry_component_decomposition.h"
#include "geometry_full_placement_domain.h"

#include "../apdp/geometry_apdp.h"
#include "../invariant/geometry_additive_invariant.h"
#include "../pruning/geometry_bumper_domain.h"
#include "../pruning/geometry_column_projection.h"
#include "../pruning/geometry_parent_hall_bound.h"
#include "../pruning/geometry_projection_reachability.h"

uint64_t clearra_geometry_catalog_identity_digest(
    const ClearraGeometryCatalogIdentity *identity);

uint64_t clearra_geometry_search_batch_id(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    uint16_t family_begin,
    uint16_t family_end,
    uint16_t partition_index,
    uint16_t partition_count);

ClearraPackingStatus clearra_geometry_authorize_full_placement_domain(
    ClearraGeometryExactCoverSearch *search,
    uint8_t state_layer,
    ClearraGeometryDomainStatus status,
    const ClearraGeometryDomainPropagation *result,
    bool *out_authorized);

ClearraPackingStatus clearra_geometry_authorize_bumper_domain(
    ClearraGeometryExactCoverSearch *search,
    uint8_t state_layer,
    ClearraGeometryBumperStatus status,
    const ClearraGeometryBumperResult *result,
    bool *out_authorized);

ClearraPackingStatus clearra_geometry_authorize_apdp_domain(
    ClearraGeometryExactCoverSearch *search,
    uint8_t state_layer,
    ClearraGeometryApdpStatus status,
    const ClearraGeometryApdpResult *result,
    bool *out_authorized);

ClearraPackingStatus clearra_geometry_authorize_hall_bound(
    ClearraGeometryExactCoverSearch *search,
    uint8_t state_layer,
    ClearraGeometryHallStatus status,
    const ClearraGeometryHallResult *result,
    bool *out_authorized);

ClearraPackingStatus clearra_geometry_authorize_column_projection(
    ClearraGeometryExactCoverSearch *search,
    uint8_t state_layer,
    ClearraGeometryColumnProjectionStatus status,
    const ClearraGeometryColumnProjectionResult *result,
    bool *out_authorized);

ClearraPackingStatus clearra_geometry_authorize_projection_reachability(
    ClearraGeometryExactCoverSearch *search,
    uint8_t state_layer,
    ClearraGeometryProjectionReachabilityStatus status,
    const ClearraGeometryProjectionReachabilityResult *result,
    bool *out_authorized);

ClearraPackingStatus clearra_geometry_authorize_additive_invariant(
    ClearraGeometryExactCoverSearch *search,
    uint8_t state_layer,
    ClearraGeometryInvariantStatus status,
    const ClearraGeometryInvariantResult *result,
    bool *out_authorized);

ClearraPackingStatus clearra_geometry_authorize_component_infeasible(
    ClearraGeometryExactCoverSearch *search,
    uint8_t state_layer,
    uint64_t remaining_cells,
    const ClearraGeometryComponentDecomposition *decomposition,
    uint64_t producer_discriminator,
    bool *out_authorized);

#endif
