#include "buildup_operation_source.h"

#include "../packing/geometry_catalog_internal.h"

clr_buildup_status clearra_buildup_operation_source_from_problem(
    const clr_buildup_problem *problem,
    ClearraBuildUpOperationSource *out_source) {
    if (problem == 0 || out_source == 0 ||
        problem->operation_set.operation_count == 0u ||
        problem->operation_set.operation_count > CLR_BUILDUP_MAX_OPERATIONS) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    *out_source = (ClearraBuildUpOperationSource){
        .problem = problem,
        .operation_count = problem->operation_set.operation_count,
        .kind = CLEARRA_BUILDUP_OPERATION_SOURCE_OWNED,
    };
    return CLR_BUILDUP_OK;
}

clr_buildup_status clearra_buildup_operation_source_from_catalog_rows(
    const clr_buildup_problem *problem,
    const ClearraGeometryCatalog *catalog,
    const uint32_t *row_ids,
    uint16_t operation_count,
    const uint8_t *representative_order_hint,
    const uint16_t *required_predecessors,
    ClearraBuildUpOperationSource *out_source) {
    if (problem == 0 || catalog == 0 || row_ids == 0 || out_source == 0 ||
        operation_count == 0u || operation_count > CLR_BUILDUP_MAX_OPERATIONS ||
        !clearra_geometry_catalog_matches_problem(catalog, &problem->packing)) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    for (uint16_t index = 0u; index < operation_count; ++index) {
        uint32_t row_id = row_ids[index];
        if (row_id >= catalog->skeleton_count ||
            clearra_geometry_catalog_representative_template(
                catalog, row_id) == 0) {
            return CLR_BUILDUP_INVALID_PROBLEM;
        }
    }
    *out_source = (ClearraBuildUpOperationSource){
        .problem = problem,
        .catalog = catalog,
        .catalog_row_ids = row_ids,
        .representative_order_hint = representative_order_hint,
        .required_predecessors = required_predecessors,
        .operation_count = operation_count,
        .kind = CLEARRA_BUILDUP_OPERATION_SOURCE_CATALOG_ROWS,
    };
    return CLR_BUILDUP_OK;
}

clr_buildup_status clearra_buildup_operation_source_operation_at(
    const ClearraBuildUpOperationSource *source,
    uint16_t operation_index,
    clr_buildup_operation *out_operation) {
    if (source == 0 || out_operation == 0 ||
        operation_index >= source->operation_count) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    if (source->kind == CLEARRA_BUILDUP_OPERATION_SOURCE_OWNED) {
        *out_operation = source->problem->operation_set.operations[operation_index];
        return CLR_BUILDUP_OK;
    }
    if (source->kind != CLEARRA_BUILDUP_OPERATION_SOURCE_CATALOG_ROWS ||
        source->catalog == 0 || source->catalog_row_ids == 0) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
    uint32_t row_id = source->catalog_row_ids[operation_index];
    if (row_id >= source->catalog->skeleton_count) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
    const ClearraInverseClearTemplate *representative =
        clearra_geometry_catalog_representative_template(
            source->catalog, row_id);
    if (representative == 0) {
        return CLR_BUILDUP_INVALID_PROBLEM;
    }
    *out_operation = (clr_buildup_operation){
        .piece = (uint8_t)source->catalog->skeleton_piece_kind[row_id],
        .rotation = representative->rotation,
        .x = representative->target_x,
        .y = representative->target_anchor_y,
        .operation_id = representative->operation_id,
        .required_deleted_row_mask = 0u,
        .mask = source->catalog->skeleton_cell_mask[row_id],
    };
    return CLR_BUILDUP_OK;
}

clr_buildup_status clearra_buildup_operation_source_order(
    const ClearraBuildUpOperationSource *source,
    ClearraBuildUpOrder *out_order) {
    if (source == 0 || out_order == 0 || source->operation_count == 0u ||
        source->operation_count > CLR_BUILDUP_MAX_OPERATIONS) {
        return CLR_BUILDUP_INVALID_ORDER;
    }
    if (source->kind == CLEARRA_BUILDUP_OPERATION_SOURCE_OWNED) {
        return clearra_buildup_order_from_problem(source->problem, out_order);
    }
    out_order->count = source->operation_count;
    uint16_t seen = 0u;
    for (uint16_t index = 0u; index < source->operation_count; ++index) {
        uint16_t operation_index = source->representative_order_hint == 0
            ? index
            : source->representative_order_hint[index];
        if (operation_index >= source->operation_count) {
            return CLR_BUILDUP_INVALID_ORDER;
        }
        uint16_t operation_bit =
            (uint16_t)(UINT16_C(1) << operation_index);
        if ((seen & operation_bit) != 0u) {
            return CLR_BUILDUP_INVALID_ORDER;
        }
        seen = (uint16_t)(seen | operation_bit);
        out_order->indices[index] = operation_index;
    }
    return CLR_BUILDUP_OK;
}

bool clearra_buildup_operation_source_has_geometry_domain(
    const ClearraBuildUpOperationSource *source,
    uint16_t operation_index) {
    if (source == 0 || operation_index >= source->operation_count) {
        return false;
    }
    if (source->kind == CLEARRA_BUILDUP_OPERATION_SOURCE_CATALOG_ROWS) {
        return true;
    }
    uint16_t operation_bit = (uint16_t)(UINT16_C(1) << operation_index);
    return (source->problem->operation_set.geometry_variant_domains &
            operation_bit) != 0u;
}

bool clearra_buildup_operation_source_may_match_clear_state(
    const ClearraBuildUpOperationSource *source,
    const ClearraBuildUpState *state,
    uint16_t operation_index) {
    if (source == 0 || state == 0 ||
        operation_index >= source->operation_count) {
        return false;
    }
    if (source->kind == CLEARRA_BUILDUP_OPERATION_SOURCE_CATALOG_ROWS) {
        if (source->catalog == 0 || source->catalog_row_ids == 0) {
            return false;
        }
        uint32_t skeleton_id = source->catalog_row_ids[operation_index];
        if (skeleton_id >= source->catalog->skeleton_count) {
            return false;
        }
        uint16_t deleted_rows = state->line_clear_state.deleted_row_mask;
        if (source->catalog->layout.height <= 6u) {
            return deleted_rows < 64u &&
                   (source->catalog->skeleton_deleted_state_bits[skeleton_id] &
                    (UINT64_C(1) << deleted_rows)) != 0u;
        }
        return clearra_geometry_catalog_skeleton_supports_clear_state(
            source->catalog, skeleton_id, deleted_rows);
    }
    if (clearra_buildup_operation_source_has_geometry_domain(
            source, operation_index)) {
        return true;
    }
    clr_buildup_operation operation;
    return clearra_buildup_operation_source_operation_at(
               source, operation_index, &operation) == CLR_BUILDUP_OK &&
           clearra_buildup_operation_matches_clear_state(state, &operation);
}
