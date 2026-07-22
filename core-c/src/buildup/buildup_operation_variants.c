#include "buildup_search_internal.h"
#include "buildup_operation_variant_cache.h"
#include "../packing/geometry_catalog_internal.h"
#include "../packing/packing_problem.h"
#include "clr_search_profile.h"

_Static_assert(
    CLR_BUILDUP_MAX_OPERATION_VARIANTS ==
        CLEARRA_PACKING_MAX_GEOMETRY_VARIANTS,
    "BuildUp and Packing must agree on the standard rotation domain bound");

static clr_buildup_status variants_from_geometry_catalog(
    const ClearraBuildUpSearchContext *context,
    const clr_buildup_operation *operation,
    uint16_t deleted_row_mask,
    clr_buildup_operation out_variants[CLR_BUILDUP_MAX_OPERATION_VARIANTS],
    uint8_t *out_count) {
    const ClearraGeometryCatalog *catalog =
        context->operation_source.catalog != 0
            ? context->operation_source.catalog
            : context->problem->geometry_catalog;
    if (catalog == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    if (!clearra_geometry_catalog_matches_problem(
            catalog, &context->problem->packing)) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
    ClearraPlacementCandidate placement_variants
        [CLEARRA_PACKING_MAX_GEOMETRY_VARIANTS];
    uint8_t placement_count = 0u;
    ClearraPackingStatus status =
        clearra_geometry_catalog_realizations_for_clear_state(
            catalog,
            operation->piece,
            operation->mask,
            deleted_row_mask,
            placement_variants,
            &placement_count);
    if (status == CLEARRA_PACKING_CAPACITY_EXCEEDED) {
        return CLR_BUILDUP_CAPACITY_EXCEEDED;
    }
    if (status != CLEARRA_PACKING_OK) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
    for (uint8_t index = 0u; index < placement_count; ++index) {
        const ClearraPlacementCandidate *placement =
            &placement_variants[index];
        out_variants[index] = (clr_buildup_operation){
            .piece = placement->piece,
            .rotation = placement->rotation,
            .x = placement->x,
            .y = placement->y,
            .operation_id = placement->operation_id,
            .required_deleted_row_mask =
                placement->required_deleted_row_mask,
            .mask = placement->mask,
        };
    }
    *out_count = placement_count;
    return placement_count == 0u ? CLR_BUILDUP_Y_ADJUSTMENT_IMPOSSIBLE
                                 : CLR_BUILDUP_OK;
}

clr_buildup_status clearra_buildup_operation_variants_for_state(
    const ClearraBuildUpSearchContext *context,
    const ClearraBuildUpState *state,
    uint16_t operation_index,
    clr_buildup_operation out_variants[CLR_BUILDUP_MAX_OPERATION_VARIANTS],
    uint8_t *out_count) {
    if (context == 0 || state == 0 || out_variants == 0 || out_count == 0 ||
        operation_index >= context->operation_source.operation_count) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    clr_buildup_operation operation;
    if (clearra_buildup_operation_source_operation_at(
            &context->operation_source,
            operation_index,
            &operation) != CLR_BUILDUP_OK) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
    if (!clearra_buildup_operation_source_has_geometry_domain(
            &context->operation_source, operation_index)) {
        out_variants[0] = operation;
        *out_count = 1u;
        return CLR_BUILDUP_OK;
    }

    uint16_t deleted_row_mask = state->line_clear_state.deleted_row_mask;
    clr_search_profile_count(
        CLR_PROFILE_BUILDUP_OPERATION_VARIANT_CACHE_LOOKUPS, 1u);
    if (clearra_buildup_operation_variant_cache_lookup(
            context->operation_variant_cache,
            context->layout,
            operation.piece,
            operation.mask,
            deleted_row_mask,
            out_variants,
            out_count)) {
        clr_search_profile_count(
            CLR_PROFILE_BUILDUP_OPERATION_VARIANT_CACHE_HITS, 1u);
        return CLR_BUILDUP_OK;
    }

    clr_search_profile_span generation_span = clr_search_profile_begin(
        CLR_PROFILE_BUILDUP_OPERATION_VARIANT_GENERATION);
    if (context->operation_source.catalog != 0 ||
        context->problem->geometry_catalog != 0) {
        clr_buildup_status status = variants_from_geometry_catalog(
            context,
            &operation,
            deleted_row_mask,
            out_variants,
            out_count);
        (void)clr_search_profile_end(generation_span, 1u);
        if (status == CLR_BUILDUP_OK) {
            clearra_buildup_operation_variant_cache_insert(
                context->operation_variant_cache,
                context->layout,
                operation.piece,
                operation.mask,
                deleted_row_mask,
                out_variants,
                *out_count);
        }
        return status;
    }

    ClearraPlacementCandidate placement_variants
        [CLR_BUILDUP_MAX_OPERATION_VARIANTS];
    uint8_t placement_count = 0u;
    ClearraPackingStatus status = clearra_placement_geometry_variants_at_deleted_rows(
        context->layout,
        operation.piece,
        operation.mask,
        deleted_row_mask,
        placement_variants,
        &placement_count);
    (void)clr_search_profile_end(generation_span, 1u);
    if (status == CLEARRA_PACKING_CAPACITY_EXCEEDED) {
        return CLR_BUILDUP_CAPACITY_EXCEEDED;
    }
    if (status != CLEARRA_PACKING_OK) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
    for (uint8_t index = 0u; index < placement_count; ++index) {
        const ClearraPlacementCandidate *placement = &placement_variants[index];
        out_variants[index] = (clr_buildup_operation){
            .piece = placement->piece,
            .rotation = placement->rotation,
            .x = placement->x,
            .y = placement->y,
            .operation_id = placement->operation_id,
            .required_deleted_row_mask = placement->required_deleted_row_mask,
            .mask = placement->mask,
        };
    }
    *out_count = placement_count;
    clearra_buildup_operation_variant_cache_insert(
        context->operation_variant_cache,
        context->layout,
        operation.piece,
        operation.mask,
        deleted_row_mask,
        out_variants,
        placement_count);
    return CLR_BUILDUP_OK;
}
