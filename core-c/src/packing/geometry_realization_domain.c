#include "geometry_catalog_internal.h"

#include "../cache/cache_identity.h"
#include "target_frame_projection.h"

static uint8_t count_deleted_rows_below(
    uint16_t deleted_row_mask,
    uint8_t row) {
    uint16_t lower_rows = row == 0u
        ? 0u
        : (uint16_t)((UINT16_C(1) << row) - UINT16_C(1));
    uint16_t value = (uint16_t)(deleted_row_mask & lower_rows);
    uint8_t count = 0u;
    while (value != 0u) {
        value = (uint16_t)(value & (uint16_t)(value - UINT16_C(1)));
        count++;
    }
    return count;
}

static bool deleted_rows_fit_layout(
    ClearraBoard64Layout layout,
    uint16_t deleted_row_mask) {
    return layout.height == 16u ||
           (deleted_row_mask >> layout.height) == 0u;
}

bool clearra_geometry_catalog_instantiate_realization(
    const ClearraGeometryCatalog *catalog,
    const ClearraInverseClearTemplate *template_value,
    uint16_t deleted_row_mask,
    ClearraConcreteRealization *out_realization) {
    if (catalog == 0 || template_value == 0 || out_realization == 0 ||
        !deleted_rows_fit_layout(catalog->layout, deleted_row_mask) ||
        (deleted_row_mask & template_value->minimum_deleted_row_mask) !=
            template_value->minimum_deleted_row_mask ||
        (deleted_row_mask & template_value->using_row_mask) != 0u ||
        template_value->target_x < 0 ||
        template_value->target_anchor_y < 0) {
        return false;
    }

    uint8_t target_anchor_y = (uint8_t)template_value->target_anchor_y;
    uint8_t deleted_below =
        count_deleted_rows_below(deleted_row_mask, target_anchor_y);
    if (deleted_below > target_anchor_y) {
        return false;
    }

    uint64_t projected_mask = 0u;
    int8_t projected_target_y = 0;
    int8_t physical_lock_y = (int8_t)(target_anchor_y - deleted_below);
    ClearraPackingStatus status = clearra_target_frame_project_lock_operation(
        catalog->layout,
        template_value->piece,
        template_value->rotation,
        template_value->target_x,
        physical_lock_y,
        deleted_row_mask,
        &projected_mask,
        &projected_target_y);
    if (status != CLEARRA_PACKING_OK ||
        projected_mask != template_value->canonical_cell_ownership ||
        projected_target_y != template_value->target_anchor_y) {
        return false;
    }

    ClearraOperation operation;
    uint64_t physical_mask = 0u;
    if (clearra_operation_from_shape(
            template_value->piece,
            template_value->rotation,
            &operation) != CLEARRA_OPERATION_OK ||
        clearra_operation_mask(
            catalog->layout,
            &operation,
            template_value->target_x,
            physical_lock_y,
            &physical_mask) != CLEARRA_OPERATION_OK) {
        return false;
    }

    uint64_t evidence = clearra_cache_key_mix_u64(
        template_value->inverse_template_id, deleted_row_mask);
    evidence = clearra_cache_key_mix_u64(evidence, physical_mask);
    evidence = clearra_cache_key_mix_u64(evidence, projected_mask);
    *out_realization = (ClearraConcreteRealization){
        .world_cell_mask = physical_mask,
        .canonical_cell_ownership = projected_mask,
        .projection_evidence_digest = evidence == 0u ? UINT64_C(1) : evidence,
        .realization_id = template_value->realization_id,
        .clear_state_deleted_row_mask = deleted_row_mask,
        .inserted_row_mask = 0u,
        .need_deleted_mask = template_value->minimum_deleted_row_mask,
        .using_row_mask = template_value->using_row_mask,
        .completed_row_mask = 0u,
        .inverse_template_id = template_value->inverse_template_id,
        .operation_id = template_value->operation_id,
        .rule_capability = template_value->rule_capability,
        .lock_x = template_value->target_x,
        .lock_y = physical_lock_y,
        .target_x = template_value->target_x,
        .target_anchor_y = template_value->target_anchor_y,
        .piece = template_value->piece,
        .rotation = template_value->rotation,
    };
    return true;
}

bool clearra_geometry_catalog_skeleton_supports_clear_state(
    const ClearraGeometryCatalog *catalog,
    uint32_t skeleton_id,
    uint16_t deleted_row_mask) {
    if (catalog == 0 || skeleton_id >= catalog->skeleton_count) {
        return false;
    }
    if (catalog->realization_deleted_state_bits != 0) {
        return deleted_row_mask < 64u &&
               (catalog->skeleton_deleted_state_bits[skeleton_id] &
                (UINT64_C(1) << deleted_row_mask)) != 0u;
    }
    uint32_t begin = catalog->skeleton_realization_offset[skeleton_id];
    uint32_t count = catalog->skeleton_realization_count[skeleton_id];
    for (uint32_t index = 0u; index < count; ++index) {
        const ClearraInverseClearTemplate *template_value =
            clearra_geometry_catalog_template_at_index(catalog, begin + index);
        ClearraConcreteRealization realization;
        if (clearra_geometry_catalog_instantiate_realization(
                catalog,
                template_value,
                deleted_row_mask,
                &realization)) {
            return true;
        }
    }
    return false;
}

bool clearra_geometry_catalog_realization_supports_clear_state(
    const ClearraGeometryCatalog *catalog,
    uint32_t realization_index,
    uint16_t deleted_row_mask) {
    if (catalog == 0 || realization_index >= catalog->realization_count) {
        return false;
    }
    if (catalog->realization_deleted_state_bits != 0) {
        return deleted_row_mask < 64u &&
               (catalog->realization_deleted_state_bits[realization_index] &
                (UINT64_C(1) << deleted_row_mask)) != 0u;
    }
    const ClearraInverseClearTemplate *template_value =
        clearra_geometry_catalog_template_at_index(catalog, realization_index);
    ClearraConcreteRealization realization;
    return clearra_geometry_catalog_instantiate_realization(
        catalog, template_value, deleted_row_mask, &realization);
}

ClearraPackingStatus clearra_geometry_catalog_realizations_for_clear_state(
    const ClearraGeometryCatalog *catalog,
    uint8_t piece,
    uint64_t canonical_cell_ownership,
    uint16_t deleted_row_mask,
    ClearraPlacementCandidate
        out_variants[CLEARRA_PACKING_MAX_GEOMETRY_VARIANTS],
    uint8_t *out_count) {
    if (catalog == 0 || out_variants == 0 || out_count == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }

    uint32_t skeleton_id = 0u;
    if (!clearra_geometry_catalog_find_skeleton(
            catalog,
            piece,
            canonical_cell_ownership,
            &skeleton_id)) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }

    *out_count = 0u;
    uint32_t begin = catalog->skeleton_realization_offset[skeleton_id];
    uint32_t count = catalog->skeleton_realization_count[skeleton_id];
    for (uint32_t index = 0u; index < count; ++index) {
        uint32_t realization_index = begin + index;
        if (!clearra_geometry_catalog_realization_supports_clear_state(
                catalog, realization_index, deleted_row_mask)) {
            continue;
        }
        const ClearraInverseClearTemplate *template_value =
            clearra_geometry_catalog_template_at_index(
                catalog, realization_index);
        if (template_value == 0) {
            *out_count = 0u;
            return CLEARRA_PACKING_INVALID_ARGUMENT;
        }
        ClearraConcreteRealization realization;
        if (!clearra_geometry_catalog_instantiate_realization(
                catalog,
                template_value,
                deleted_row_mask,
                &realization)) {
            continue;
        }
        ClearraPlacementCandidate candidate = {
            .piece = realization.piece,
            .rotation = realization.rotation,
            .x = realization.target_x,
            .y = realization.target_anchor_y,
            .operation_id = realization.operation_id,
            .required_deleted_row_mask = deleted_row_mask,
            .mask = realization.canonical_cell_ownership,
        };
        bool duplicate = false;
        for (uint8_t existing = 0u; existing < *out_count; ++existing) {
            const ClearraPlacementCandidate *value = &out_variants[existing];
            if (value->piece == candidate.piece &&
                value->rotation == candidate.rotation &&
                value->x == candidate.x && value->y == candidate.y &&
                value->operation_id == candidate.operation_id &&
                value->required_deleted_row_mask ==
                    candidate.required_deleted_row_mask &&
                value->mask == candidate.mask) {
                duplicate = true;
                break;
            }
        }
        if (duplicate) {
            continue;
        }
        if (*out_count == CLEARRA_PACKING_MAX_GEOMETRY_VARIANTS) {
            *out_count = 0u;
            return CLEARRA_PACKING_CAPACITY_EXCEEDED;
        }
        out_variants[*out_count] = candidate;
        (*out_count)++;
    }
    return CLEARRA_PACKING_OK;
}
