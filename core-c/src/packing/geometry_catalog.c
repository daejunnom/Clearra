#include "geometry_catalog_internal.h"

#include "../apdp/geometry_apdp.h"
#include "../cache/cache_identity.h"
#include "../invariant/geometry_additive_invariant.h"
#include "clr_execution_control.h"

#include <limits.h>
#include <stdlib.h>
#include <string.h>

typedef struct ClearraCatalogBuildContext {
    ClearraGeometryCatalog *catalog;
} ClearraCatalogBuildContext;

static uint8_t popcount64(uint64_t value) {
    uint8_t count = 0u;
    while (value != 0u) {
        value &= value - UINT64_C(1);
        count++;
    }
    return count;
}

static uint16_t row_mask_for_cells(
    ClearraBoard64Layout layout,
    uint64_t cells) {
    uint16_t rows = 0u;
    for (uint8_t row = 0u; row < layout.height; ++row) {
        uint64_t row_mask = 0u;
        if (clearra_board64_row_mask(layout, row, &row_mask) !=
            CLEARRA_BOARD64_OK) {
            return 0u;
        }
        if ((cells & row_mask) != 0u) {
            rows = (uint16_t)(rows | (uint16_t)(UINT16_C(1) << row));
        }
    }
    return rows;
}

static bool catalog_can_add(
    const ClearraGeometryCatalog *catalog,
    size_t bytes) {
    return catalog != 0 && bytes <= SIZE_MAX - catalog->resident_bytes &&
           (catalog->max_resident_bytes == SIZE_MAX ||
            catalog->resident_bytes + bytes <= catalog->max_resident_bytes);
}

static void *catalog_allocate(
    ClearraGeometryCatalog *catalog,
    size_t bytes) {
    if (bytes == 0u || !catalog_can_add(catalog, bytes)) {
        return 0;
    }
    void *allocation = malloc(bytes);
    if (allocation != 0) {
        catalog->resident_bytes += bytes;
    }
    return allocation;
}

static ClearraPackingStatus append_realization(
    void *context_ptr,
    const ClearraPlacementCandidate *candidate) {
    ClearraCatalogBuildContext *context =
        (ClearraCatalogBuildContext *)context_ptr;
    if (context == 0 || context->catalog == 0 || candidate == 0 ||
        popcount64(candidate->mask) != CLEARRA_TETROMINO_AREA) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    ClearraGeometryCatalog *catalog = context->catalog;
    ClearraRealizationChunk *chunk = catalog->realization_tail;
    if (chunk == 0 || chunk->count == CLEARRA_REALIZATION_CHUNK_CAPACITY) {
        chunk = (ClearraRealizationChunk *)catalog_allocate(
            catalog, sizeof(*chunk));
        if (chunk == 0) {
            return CLEARRA_PACKING_CAPACITY_EXCEEDED;
        }
        chunk->next = 0;
        chunk->count = 0u;
        if (catalog->realization_tail == 0) {
            catalog->realization_chunks = chunk;
        } else {
            catalog->realization_tail->next = chunk;
        }
        catalog->realization_tail = chunk;
    }
    ClearraInverseClearTemplate *template_value =
        &chunk->items[chunk->count++];
    *template_value = (ClearraInverseClearTemplate){
        .canonical_cell_ownership = candidate->mask,
        .realization_id = 0u,
        .minimum_deleted_row_mask = candidate->required_deleted_row_mask,
        .using_row_mask = row_mask_for_cells(catalog->layout, candidate->mask),
        .inverse_template_id = candidate->operation_id,
        .operation_id = 0u,
        .rule_capability = catalog->identity.rule_capability_id,
        .target_x = candidate->x,
        .target_anchor_y = candidate->y,
        .piece = candidate->piece,
        .rotation = candidate->rotation,
    };
    catalog->realization_payload_count++;
    return CLEARRA_PACKING_OK;
}

static int compare_realization_values(
    const ClearraInverseClearTemplate *left,
    const ClearraInverseClearTemplate *right) {
#define COMPARE_FIELD(field)                         \
    if (left->field != right->field) {               \
        return left->field < right->field ? -1 : 1;  \
    }
    COMPARE_FIELD(piece)
    COMPARE_FIELD(canonical_cell_ownership)
    COMPARE_FIELD(minimum_deleted_row_mask)
    COMPARE_FIELD(rotation)
    COMPARE_FIELD(target_x)
    COMPARE_FIELD(target_anchor_y)
    COMPARE_FIELD(inverse_template_id)
#undef COMPARE_FIELD
    return 0;
}

static int compare_realization_refs(const void *left_ptr, const void *right_ptr) {
    const ClearraInverseClearTemplate *left =
        *(ClearraInverseClearTemplate *const *)left_ptr;
    const ClearraInverseClearTemplate *right =
        *(ClearraInverseClearTemplate *const *)right_ptr;
    return compare_realization_values(left, right);
}

static bool same_realization(
    const ClearraInverseClearTemplate *left,
    const ClearraInverseClearTemplate *right) {
    return compare_realization_values(left, right) == 0;
}

static bool same_skeleton(
    const ClearraInverseClearTemplate *left,
    const ClearraInverseClearTemplate *right) {
    return left->piece == right->piece &&
           left->canonical_cell_ownership == right->canonical_cell_ownership;
}

static ClearraPackingStatus build_sorted_realization_refs(
    ClearraGeometryCatalog *catalog) {
    if (catalog->realization_payload_count == 0u) {
        catalog->realization_count = 0u;
        return CLEARRA_PACKING_OK;
    }
    if (catalog->realization_payload_count >
        SIZE_MAX / sizeof(*catalog->realization_refs)) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    size_t ref_bytes = (size_t)catalog->realization_payload_count *
                       sizeof(*catalog->realization_refs);
    catalog->realization_refs =
        (ClearraInverseClearTemplate **)catalog_allocate(catalog, ref_bytes);
    if (catalog->realization_refs == 0) {
        return CLEARRA_PACKING_CAPACITY_EXCEEDED;
    }
    uint32_t cursor = 0u;
    for (ClearraRealizationChunk *chunk = catalog->realization_chunks;
         chunk != 0;
         chunk = chunk->next) {
        for (uint32_t index = 0u; index < chunk->count; ++index) {
            catalog->realization_refs[cursor++] = &chunk->items[index];
        }
    }
    if (cursor != catalog->realization_payload_count) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    qsort(
        catalog->realization_refs,
        cursor,
        sizeof(*catalog->realization_refs),
        compare_realization_refs);

    uint32_t unique_count = 0u;
    for (uint32_t index = 0u; index < cursor; ++index) {
        if (unique_count == 0u ||
            !same_realization(
                catalog->realization_refs[index],
                catalog->realization_refs[unique_count - 1u])) {
            catalog->realization_refs[unique_count++] =
                catalog->realization_refs[index];
        }
    }
    catalog->realization_count = unique_count;
    if (unique_count > UINT16_MAX) {
        return CLEARRA_PACKING_CAPACITY_EXCEEDED;
    }
    for (uint32_t index = 0u; index < unique_count; ++index) {
        catalog->realization_refs[index]->realization_id = index + 1u;
        catalog->realization_refs[index]->operation_id =
            (uint16_t)(index + 1u);
    }
    return CLEARRA_PACKING_OK;
}

static void finalize_realization_table_digest(
    ClearraGeometryCatalog *catalog) {
    uint64_t digest = UINT64_C(1469598103934665603);
    for (uint32_t index = 0u; index < catalog->realization_count; ++index) {
        const ClearraInverseClearTemplate *realization =
            catalog->realization_refs[index];
        digest = clearra_cache_key_mix_u64(
            digest, realization->realization_id);
        digest = clearra_cache_key_mix_u64(digest, realization->piece);
        digest = clearra_cache_key_mix_u64(
            digest, realization->canonical_cell_ownership);
        digest = clearra_cache_key_mix_u64(
            digest, realization->minimum_deleted_row_mask);
        digest = clearra_cache_key_mix_u64(digest, realization->rotation);
        digest = clearra_cache_key_mix_u64(
            digest, (uint8_t)realization->target_x);
        digest = clearra_cache_key_mix_u64(
            digest, (uint8_t)realization->target_anchor_y);
        digest = clearra_cache_key_mix_u64(
            digest,
            catalog->realization_deleted_state_bits == 0
                ? 0u
                : catalog->realization_deleted_state_bits[index]);
        digest = clearra_cache_key_mix_u64(
            digest, realization->operation_id);
        digest = clearra_cache_key_mix_u64(
            digest, realization->inverse_template_id);
    }
    catalog->identity.realization_table_digest =
        digest == 0u ? UINT64_C(1) : digest;
}

static ClearraPackingStatus allocate_skeleton_arrays(
    ClearraGeometryCatalog *catalog,
    uint32_t skeleton_count) {
    if (skeleton_count == 0u) {
        return CLEARRA_PACKING_OK;
    }
    if (skeleton_count > SIZE_MAX / sizeof(uint64_t) ||
        skeleton_count > SIZE_MAX / sizeof(uint32_t) ||
        skeleton_count > SIZE_MAX / sizeof(uint16_t)) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    catalog->skeleton_piece_kind =
        (uint32_t *)catalog_allocate(catalog, skeleton_count * sizeof(uint32_t));
    catalog->skeleton_cell_mask =
        (uint64_t *)catalog_allocate(catalog, skeleton_count * sizeof(uint64_t));
    catalog->skeleton_realization_offset =
        (uint32_t *)catalog_allocate(catalog, skeleton_count * sizeof(uint32_t));
    catalog->skeleton_realization_count =
        (uint32_t *)catalog_allocate(catalog, skeleton_count * sizeof(uint32_t));
    catalog->skeleton_parent_row_id =
        (uint32_t *)catalog_allocate(catalog, skeleton_count * sizeof(uint32_t));
    catalog->skeleton_deleted_state_bits =
        (uint64_t *)catalog_allocate(catalog, skeleton_count * sizeof(uint64_t));
    catalog->skeleton_using_row_mask =
        (uint16_t *)catalog_allocate(catalog, skeleton_count * sizeof(uint16_t));
    catalog->skeleton_required_deleted_rows =
        (uint16_t *)catalog_allocate(catalog, skeleton_count * sizeof(uint16_t));
    return catalog->skeleton_piece_kind != 0 &&
                   catalog->skeleton_cell_mask != 0 &&
                   catalog->skeleton_realization_offset != 0 &&
                   catalog->skeleton_realization_count != 0 &&
                   catalog->skeleton_parent_row_id != 0 &&
                   catalog->skeleton_deleted_state_bits != 0 &&
                   catalog->skeleton_using_row_mask != 0 &&
                   catalog->skeleton_required_deleted_rows != 0
        ? CLEARRA_PACKING_OK
        : CLEARRA_PACKING_CAPACITY_EXCEEDED;
}

static ClearraPackingStatus build_skeletons(
    ClearraGeometryCatalog *catalog) {
    uint32_t skeleton_count = 0u;
    for (uint32_t index = 0u; index < catalog->realization_count; ++index) {
        if (index == 0u ||
            !same_skeleton(
                catalog->realization_refs[index - 1u],
                catalog->realization_refs[index])) {
            skeleton_count++;
        }
    }
    ClearraPackingStatus status =
        allocate_skeleton_arrays(catalog, skeleton_count);
    if (status != CLEARRA_PACKING_OK) {
        return status;
    }

    catalog->skeleton_count = skeleton_count;
    if (skeleton_count == 0u) {
        return CLEARRA_PACKING_OK;
    }
    if (catalog->layout.height <= 6u) {
        if (catalog->realization_count >
            SIZE_MAX / sizeof(*catalog->realization_deleted_state_bits)) {
            return CLEARRA_PACKING_INVALID_ARGUMENT;
        }
        catalog->realization_deleted_state_bits = (uint64_t *)catalog_allocate(
            catalog,
            (size_t)catalog->realization_count *
                sizeof(*catalog->realization_deleted_state_bits));
        if (catalog->realization_deleted_state_bits == 0) {
            return CLEARRA_PACKING_CAPACITY_EXCEEDED;
        }
    }
    uint32_t skeleton_id = 0u;
    uint32_t first = 0u;
    while (first < catalog->realization_count) {
        uint32_t end = first + 1u;
        while (end < catalog->realization_count &&
               same_skeleton(
                   catalog->realization_refs[first],
                   catalog->realization_refs[end])) {
            end++;
        }
        const ClearraInverseClearTemplate *representative =
            catalog->realization_refs[first];
        catalog->skeleton_piece_kind[skeleton_id] = representative->piece;
        catalog->skeleton_cell_mask[skeleton_id] =
            representative->canonical_cell_ownership;
        catalog->skeleton_realization_offset[skeleton_id] = first;
        catalog->skeleton_realization_count[skeleton_id] = end - first;
        catalog->skeleton_parent_row_id[skeleton_id] = skeleton_id;
        catalog->skeleton_using_row_mask[skeleton_id] = row_mask_for_cells(
            catalog->layout, representative->canonical_cell_ownership);
        uint64_t deleted_state_bits = 0u;
        uint16_t required_deleted_rows =
            representative->minimum_deleted_row_mask;
        for (uint32_t realization_index = first;
             realization_index < end;
             ++realization_index) {
            const ClearraInverseClearTemplate *realization =
                catalog->realization_refs[realization_index];
            if (realization->piece != representative->piece ||
                realization->canonical_cell_ownership !=
                    representative->canonical_cell_ownership) {
                return CLEARRA_PACKING_INVALID_ARGUMENT;
            }
            required_deleted_rows = (uint16_t)(
                required_deleted_rows &
                realization->minimum_deleted_row_mask);
        }
        if (catalog->realization_deleted_state_bits != 0) {
            uint16_t clear_state_count =
                (uint16_t)(UINT16_C(1) << catalog->layout.height);
            for (uint32_t realization_index = first;
                 realization_index < end;
                 ++realization_index) {
                const ClearraInverseClearTemplate *realization =
                    catalog->realization_refs[realization_index];
                uint64_t realization_states = 0u;
                for (uint16_t deleted_rows = 0u;
                     deleted_rows < clear_state_count;
                     ++deleted_rows) {
                    ClearraConcreteRealization concrete;
                    if (clearra_geometry_catalog_instantiate_realization(
                            catalog,
                            realization,
                            deleted_rows,
                            &concrete)) {
                        realization_states |= UINT64_C(1) << deleted_rows;
                    }
                }
                catalog->realization_deleted_state_bits[realization_index] =
                    realization_states;
                deleted_state_bits |= realization_states;
            }
        }
        catalog->skeleton_deleted_state_bits[skeleton_id] =
            deleted_state_bits;
        catalog->skeleton_required_deleted_rows[skeleton_id] =
            required_deleted_rows;
        skeleton_id++;
        first = end;
    }
    return CLEARRA_PACKING_OK;
}

static ClearraGeometryColumnSignature column_signature_for_cells(
    ClearraBoard64Layout layout,
    uint64_t cells) {
    uint8_t counts[16] = {0u};
    while (cells != 0u) {
        uint64_t bit = cells & (~cells + UINT64_C(1));
        uint8_t cell = 0u;
        for (uint64_t cursor = bit;
             (cursor & UINT64_C(1)) == 0u;
             cursor >>= 1u) {
            cell++;
        }
        counts[cell % layout.width]++;
        cells &= ~bit;
    }

    ClearraGeometryColumnSignature signature = {0u, 0u};
    for (uint8_t column = 0u; column < layout.width; ++column) {
        if (column < 12u) {
            signature.low |= (uint64_t)counts[column] << (column * 5u);
        } else {
            signature.high |=
                (uint32_t)counts[column] << ((column - 12u) * 5u);
        }
    }
    return signature;
}

static int compare_piece_projections(
    const void *left_ptr,
    const void *right_ptr) {
    const ClearraGeometryPieceProjection *left =
        (const ClearraGeometryPieceProjection *)left_ptr;
    const ClearraGeometryPieceProjection *right =
        (const ClearraGeometryPieceProjection *)right_ptr;
    if (left->piece != right->piece) {
        return left->piece < right->piece ? -1 : 1;
    }
    if (left->signature.high != right->signature.high) {
        return left->signature.high < right->signature.high ? -1 : 1;
    }
    if (left->signature.low != right->signature.low) {
        return left->signature.low < right->signature.low ? -1 : 1;
    }
    return 0;
}

static ClearraPackingStatus build_column_projection_catalog(
    ClearraGeometryCatalog *catalog) {
    if (catalog->layout.width > 16u) {
        return CLEARRA_PACKING_OK;
    }
    uint32_t skeleton_count = catalog->skeleton_count;
    if (skeleton_count == 0u) {
        return CLEARRA_PACKING_OK;
    }
    if (skeleton_count > SIZE_MAX / sizeof(uint64_t) ||
        skeleton_count > SIZE_MAX / sizeof(uint32_t) ||
        skeleton_count > SIZE_MAX / sizeof(ClearraGeometryPieceProjection)) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    catalog->skeleton_column_projection_low = (uint64_t *)catalog_allocate(
        catalog, (size_t)skeleton_count * sizeof(uint64_t));
    catalog->skeleton_column_projection_high = (uint32_t *)catalog_allocate(
        catalog, (size_t)skeleton_count * sizeof(uint32_t));
    catalog->piece_column_projections =
        (ClearraGeometryPieceProjection *)catalog_allocate(
            catalog,
            (size_t)skeleton_count *
                sizeof(ClearraGeometryPieceProjection));
    if (catalog->skeleton_column_projection_low == 0 ||
        catalog->skeleton_column_projection_high == 0 ||
        catalog->piece_column_projections == 0) {
        return CLEARRA_PACKING_CAPACITY_EXCEEDED;
    }

    for (uint32_t row_id = 0u; row_id < skeleton_count; ++row_id) {
        ClearraGeometryColumnSignature signature = column_signature_for_cells(
            catalog->layout, catalog->skeleton_cell_mask[row_id]);
        catalog->skeleton_column_projection_low[row_id] = signature.low;
        catalog->skeleton_column_projection_high[row_id] = signature.high;
        catalog->piece_column_projections[row_id] =
            (ClearraGeometryPieceProjection){
                .signature = signature,
                .piece = (uint8_t)catalog->skeleton_piece_kind[row_id],
            };
    }
    qsort(
        catalog->piece_column_projections,
        skeleton_count,
        sizeof(*catalog->piece_column_projections),
        compare_piece_projections);

    uint32_t source = 0u;
    uint32_t destination = 0u;
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        catalog->piece_projection_offsets[piece] = destination;
        while (source < skeleton_count &&
               catalog->piece_column_projections[source].piece < piece) {
            source++;
        }
        while (source < skeleton_count &&
               catalog->piece_column_projections[source].piece == piece) {
            ClearraGeometryPieceProjection value =
                catalog->piece_column_projections[source++];
            if (destination != catalog->piece_projection_offsets[piece] &&
                compare_piece_projections(
                    &catalog->piece_column_projections[destination - 1u],
                    &value) == 0) {
                continue;
            }
            catalog->piece_column_projections[destination++] = value;
        }
    }
    catalog->piece_projection_offsets[CLR_STANDARD_PIECE_KIND_COUNT] =
        destination;
    catalog->piece_column_projection_count = destination;
    return CLEARRA_PACKING_OK;
}

static uint8_t single_bit_index(uint64_t bit) {
    uint8_t index = 0u;
    while ((bit & UINT64_C(1)) == 0u) {
        bit >>= 1u;
        index++;
    }
    return index;
}

static ClearraPackingStatus build_cell_support(
    ClearraGeometryCatalog *catalog) {
    uint32_t counts[64] = {0u};
    for (uint32_t row_id = 0u; row_id < catalog->skeleton_count; ++row_id) {
        uint64_t cells = catalog->skeleton_cell_mask[row_id];
        while (cells != 0u) {
            uint64_t bit = cells & (~cells + UINT64_C(1));
            uint8_t cell = single_bit_index(bit);
            if (cell >= catalog->layout.cell_count || counts[cell] == UINT32_MAX) {
                return CLEARRA_PACKING_CAPACITY_EXCEEDED;
            }
            counts[cell]++;
            cells &= ~bit;
        }
    }

    uint32_t total = 0u;
    for (uint8_t cell = 0u; cell < catalog->layout.cell_count; ++cell) {
        catalog->cell_support_offsets[cell] = total;
        if (counts[cell] > UINT32_MAX - total) {
            return CLEARRA_PACKING_CAPACITY_EXCEEDED;
        }
        total += counts[cell];
    }
    catalog->cell_support_offsets[catalog->layout.cell_count] = total;
    catalog->support_entry_count = total;
    if (total == 0u) {
        catalog->identity.support_table_digest = UINT64_C(1);
        return CLEARRA_PACKING_OK;
    }
    if (total > SIZE_MAX / sizeof(uint32_t)) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    catalog->cell_support_row_ids =
        (uint32_t *)catalog_allocate(catalog, total * sizeof(uint32_t));
    if (catalog->cell_support_row_ids == 0) {
        return CLEARRA_PACKING_CAPACITY_EXCEEDED;
    }

    uint32_t cursors[64];
    for (uint8_t cell = 0u; cell < catalog->layout.cell_count; ++cell) {
        cursors[cell] = catalog->cell_support_offsets[cell];
    }
    uint64_t support_digest = UINT64_C(1469598103934665603);
    for (uint32_t row_id = 0u; row_id < catalog->skeleton_count; ++row_id) {
        support_digest = clearra_cache_key_mix_u64(
            support_digest, catalog->skeleton_apdp_support_flags[row_id]);
        support_digest = clearra_cache_key_mix_u64(
            support_digest,
            catalog->skeleton_column_projection_low == 0
                ? 0u
                : catalog->skeleton_column_projection_low[row_id]);
        support_digest = clearra_cache_key_mix_u64(
            support_digest,
            catalog->skeleton_column_projection_high == 0
                ? 0u
                : catalog->skeleton_column_projection_high[row_id]);
        uint64_t cells = catalog->skeleton_cell_mask[row_id];
        while (cells != 0u) {
            uint64_t bit = cells & (~cells + UINT64_C(1));
            uint8_t cell = single_bit_index(bit);
            catalog->cell_support_row_ids[cursors[cell]++] = row_id;
            support_digest = clearra_cache_key_mix_u64(support_digest, cell);
            support_digest = clearra_cache_key_mix_u64(support_digest, row_id);
            cells &= ~bit;
        }
    }
    catalog->identity.support_table_digest =
        support_digest == 0u ? UINT64_C(1) : support_digest;
    return CLEARRA_PACKING_OK;
}

static ClearraPackingStatus build_additive_signatures(
    ClearraGeometryCatalog *catalog) {
    if (catalog->skeleton_count == 0u) {
        return CLEARRA_PACKING_OK;
    }
    if (catalog->skeleton_count >
        SIZE_MAX / CLEARRA_ADDITIVE_SIGNATURE_BANK_COUNT) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    size_t bytes = (size_t)catalog->skeleton_count *
                   CLEARRA_ADDITIVE_SIGNATURE_BANK_COUNT;
    catalog->skeleton_additive_signatures =
        (uint8_t *)catalog_allocate(catalog, bytes);
    if (catalog->skeleton_additive_signatures == 0) {
        return CLEARRA_PACKING_CAPACITY_EXCEEDED;
    }
    return clearra_geometry_additive_invariant_compile_signatures(
               catalog->layout,
               catalog->skeleton_cell_mask,
               catalog->skeleton_count,
               catalog->skeleton_additive_signatures)
        ? CLEARRA_PACKING_OK
        : CLEARRA_PACKING_INVALID_ARGUMENT;
}

static ClearraPackingStatus build_apdp_support_flags(
    ClearraGeometryCatalog *catalog) {
    if (catalog->skeleton_count == 0u) {
        return CLEARRA_PACKING_OK;
    }
    catalog->skeleton_apdp_support_flags = (uint8_t *)catalog_allocate(
        catalog, catalog->skeleton_count * sizeof(uint8_t));
    if (catalog->skeleton_apdp_support_flags == 0) {
        return CLEARRA_PACKING_CAPACITY_EXCEEDED;
    }
    return clearra_geometry_apdp_compile_support_flags(
               catalog->layout,
               catalog->skeleton_cell_mask,
               catalog->skeleton_count,
               catalog->skeleton_apdp_support_flags)
        ? CLEARRA_PACKING_OK
        : CLEARRA_PACKING_INVALID_ARGUMENT;
}

static ClearraPackingStatus catalog_layout(
    const clr_packing_problem *problem,
    ClearraBoard64Layout *out_layout) {
    uint16_t height = problem->board.search_height == 0u
        ? problem->board.visible_height
        : problem->board.search_height;
    if (problem->board.width > UINT8_MAX || height > 16u) {
        return CLEARRA_PACKING_INVALID_LAYOUT;
    }
    return clearra_board64_make_layout(
               (uint8_t)problem->board.width,
               (uint8_t)height,
               out_layout) == CLEARRA_BOARD64_OK
        ? CLEARRA_PACKING_OK
        : CLEARRA_PACKING_INVALID_LAYOUT;
}

static void initialize_identity(
    ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem) {
    uint64_t layout_id = UINT64_C(1469598103934665603);
    layout_id = clearra_cache_key_mix_u64(layout_id, catalog->layout.width);
    layout_id = clearra_cache_key_mix_u64(layout_id, catalog->layout.height);
    layout_id = clearra_cache_key_mix_u64(layout_id, catalog->layout.cell_count);
    catalog->identity.board_layout_id = layout_id;
    catalog->identity.compact_universe_digest = clearra_cache_key_mix_u64(
        clearra_cache_key_mix_u64(layout_id, problem->board.initial_mask),
        problem->required_fill_mask);
    catalog->identity.target_geometry_digest = clearra_cache_key_mix_u64(
        clearra_cache_key_mix_u64(
            problem->goal_region_mask, problem->required_fill_mask),
        problem->forbidden_mask);
    catalog->identity.piece_catalog_id =
        problem->rule.piece_set_profile_id;
    catalog->identity.skeleton_projection_version =
        CLEARRA_SKELETON_PROJECTION_VERSION;
    catalog->identity.rule_capability_id = clearra_cache_key_mix_u64(
        problem->rule.rule_profile_id, problem->rule.kick_profile_id);
}

static void destroy_catalog(ClearraGeometryCatalog *catalog) {
    if (catalog == 0) {
        return;
    }
    ClearraRealizationChunk *chunk = catalog->realization_chunks;
    while (chunk != 0) {
        ClearraRealizationChunk *next = chunk->next;
        free(chunk);
        chunk = next;
    }
    free(catalog->skeleton_piece_kind);
    free(catalog->skeleton_cell_mask);
    free(catalog->skeleton_realization_offset);
    free(catalog->skeleton_realization_count);
    free(catalog->skeleton_parent_row_id);
    free(catalog->skeleton_deleted_state_bits);
    free(catalog->realization_deleted_state_bits);
    free(catalog->skeleton_using_row_mask);
    free(catalog->skeleton_required_deleted_rows);
    free(catalog->skeleton_additive_signatures);
    free(catalog->skeleton_apdp_support_flags);
    free(catalog->skeleton_column_projection_low);
    free(catalog->skeleton_column_projection_high);
    free(catalog->piece_column_projections);
    free(catalog->realization_refs);
    free(catalog->cell_support_row_ids);
    free(catalog);
}

ClearraPackingStatus clearra_geometry_catalog_compile(
    const clr_packing_problem *problem,
    clr_resource_report *out_resource_report,
    clr_pruning_evidence_policy evidence_policy,
    clr_pruning_proof_ledger *out_pruning_ledger,
    ClearraGeometryCatalog **out_catalog) {
    if (problem == 0 || out_resource_report == 0 || out_pruning_ledger == 0 ||
        out_catalog == 0 || !clr_packing_problem_is_valid(problem)) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    *out_catalog = 0;
    clr_resource_report_clear(out_resource_report);
    if (clr_pruning_proof_ledger_init_with_policy(
            out_pruning_ledger, evidence_policy) != CLR_PRUNING_OK) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }

    ClearraGeometryCatalog *catalog =
        (ClearraGeometryCatalog *)malloc(sizeof(*catalog));
    if (catalog == 0) {
        clr_resource_report_mark_truncated(
            out_resource_report, CLR_RESOURCE_TRUNCATION_MEMORY_EXCEEDED);
        return CLEARRA_PACKING_CAPACITY_EXCEEDED;
    }
    *catalog = (ClearraGeometryCatalog){0};
    catalog->resident_bytes = sizeof(*catalog);
    if (problem->budget.has_max_memory_mib == 0u) {
        catalog->max_resident_bytes = SIZE_MAX;
    } else if ((size_t)problem->budget.max_memory_mib >
               SIZE_MAX / ((size_t)1024u * (size_t)1024u)) {
        catalog->max_resident_bytes = SIZE_MAX;
    } else {
        catalog->max_resident_bytes =
            (size_t)problem->budget.max_memory_mib *
            (size_t)1024u * (size_t)1024u;
    }
    if (catalog->max_resident_bytes < catalog->resident_bytes) {
        destroy_catalog(catalog);
        clr_resource_report_mark_truncated(
            out_resource_report, CLR_RESOURCE_TRUNCATION_MEMORY_EXCEEDED);
        return CLEARRA_PACKING_CAPACITY_EXCEEDED;
    }
    ClearraPackingStatus status = catalog_layout(problem, &catalog->layout);
    if (status != CLEARRA_PACKING_OK) {
        destroy_catalog(catalog);
        return status;
    }
    catalog->initial_board = problem->board.initial_mask;
    catalog->goal_region_mask = problem->goal_region_mask;
    catalog->required_fill_mask =
        problem->required_fill_mask & ~problem->board.initial_mask;
    catalog->forbidden_mask = problem->forbidden_mask;
    initialize_identity(catalog, problem);

    clr_static_prune_context prune_context;
    status = clearra_packing_prune_context_from_problem(
        problem, &prune_context);
    if (status != CLEARRA_PACKING_OK) {
        destroy_catalog(catalog);
        return status;
    }
    ClearraCatalogBuildContext build = {.catalog = catalog};
    uint32_t cancellation_poll_counter = 0u;
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        if (clr_execution_control_poll(&cancellation_poll_counter)) {
            destroy_catalog(catalog);
            clr_resource_report_mark_truncated(
                out_resource_report, CLR_RESOURCE_TRUNCATION_CANCELLED);
            return CLEARRA_PACKING_CANCELLED;
        }
        status = clearra_placement_candidates_visit_with_pruning_ledger(
            catalog->layout,
            catalog->initial_board,
            catalog->goal_region_mask,
            piece,
            &prune_context,
            out_pruning_ledger,
            append_realization,
            &build);
        if (status != CLEARRA_PACKING_OK) {
            destroy_catalog(catalog);
            if (status == CLEARRA_PACKING_CAPACITY_EXCEEDED) {
                clr_resource_report_mark_truncated(
                    out_resource_report,
                    CLR_RESOURCE_TRUNCATION_MEMORY_EXCEEDED);
            }
            return status;
        }
    }

    status = build_sorted_realization_refs(catalog);
    if (status == CLEARRA_PACKING_OK) {
        status = build_skeletons(catalog);
    }
    if (status == CLEARRA_PACKING_OK) {
        finalize_realization_table_digest(catalog);
    }
    if (status == CLEARRA_PACKING_OK) {
        status = build_column_projection_catalog(catalog);
    }
    if (status == CLEARRA_PACKING_OK) {
        status = build_apdp_support_flags(catalog);
    }
    if (status == CLEARRA_PACKING_OK) {
        status = build_additive_signatures(catalog);
    }
    if (status == CLEARRA_PACKING_OK) {
        status = build_cell_support(catalog);
    }
    if (status != CLEARRA_PACKING_OK) {
        destroy_catalog(catalog);
        if (status == CLEARRA_PACKING_CAPACITY_EXCEEDED) {
            clr_resource_report_mark_truncated(
                out_resource_report, CLR_RESOURCE_TRUNCATION_MEMORY_EXCEEDED);
        }
        return status;
    }

    clr_resource_report_observe_hash_buckets(
        out_resource_report, catalog->support_entry_count);
    clr_resource_report_observe_cpu_bytes(
        out_resource_report, catalog->resident_bytes);
    *out_catalog = catalog;
    return CLEARRA_PACKING_OK;
}

void clearra_geometry_catalog_release(ClearraGeometryCatalog **catalog) {
    if (catalog == 0 || *catalog == 0) {
        return;
    }
    destroy_catalog(*catalog);
    *catalog = 0;
}

const ClearraGeometryCatalogIdentity *clearra_geometry_catalog_identity(
    const ClearraGeometryCatalog *catalog) {
    return catalog == 0 ? 0 : &catalog->identity;
}

size_t clearra_geometry_catalog_resident_bytes(
    const ClearraGeometryCatalog *catalog) {
    return catalog == 0 ? 0u : catalog->resident_bytes;
}

uint32_t clearra_geometry_catalog_skeleton_count(
    const ClearraGeometryCatalog *catalog) {
    return catalog == 0 ? 0u : catalog->skeleton_count;
}

uint32_t clearra_geometry_catalog_realization_count(
    const ClearraGeometryCatalog *catalog) {
    return catalog == 0 ? 0u : catalog->realization_count;
}

bool clearra_geometry_catalog_borrow_view(
    const ClearraGeometryCatalog *catalog,
    ClearraGeometryCatalogView *out_view) {
    if (catalog == 0 || out_view == 0) {
        return false;
    }
    *out_view = (ClearraGeometryCatalogView){
        .identity = catalog->identity,
        .skeleton_cell_masks = catalog->skeleton_cell_mask,
        .skeleton_piece_kinds = catalog->skeleton_piece_kind,
        .skeleton_realization_offsets = catalog->skeleton_realization_offset,
        .skeleton_realization_counts = catalog->skeleton_realization_count,
        .cell_support_offsets = catalog->cell_support_offsets,
        .cell_support_row_ids = catalog->cell_support_row_ids,
        .skeleton_count = catalog->skeleton_count,
        .realization_count = catalog->realization_count,
        .support_entry_count = catalog->support_entry_count,
        .cell_count = catalog->layout.cell_count,
    };
    return true;
}

bool clearra_geometry_catalog_matches_problem(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem) {
    if (catalog == 0 || problem == 0) {
        return false;
    }
    uint16_t height = problem->board.search_height == 0u
        ? problem->board.visible_height
        : problem->board.search_height;
    return catalog->layout.width == problem->board.width &&
           catalog->layout.height == height &&
           catalog->initial_board == problem->board.initial_mask &&
           catalog->goal_region_mask == problem->goal_region_mask &&
           catalog->required_fill_mask ==
               (problem->required_fill_mask & ~problem->board.initial_mask) &&
           catalog->forbidden_mask == problem->forbidden_mask &&
           catalog->identity.piece_catalog_id ==
               problem->rule.piece_set_profile_id &&
           catalog->identity.rule_capability_id == clearra_cache_key_mix_u64(
               problem->rule.rule_profile_id, problem->rule.kick_profile_id);
}

bool clearra_geometry_catalog_find_skeleton(
    const ClearraGeometryCatalog *catalog,
    uint8_t piece,
    uint64_t canonical_cell_ownership,
    uint32_t *out_skeleton_id) {
    if (catalog == 0 || out_skeleton_id == 0) {
        return false;
    }
    uint32_t low = 0u;
    uint32_t high = catalog->skeleton_count;
    while (low < high) {
        uint32_t middle = low + (high - low) / 2u;
        uint32_t middle_piece = catalog->skeleton_piece_kind[middle];
        uint64_t middle_cells = catalog->skeleton_cell_mask[middle];
        if (middle_piece < piece ||
            (middle_piece == piece && middle_cells < canonical_cell_ownership)) {
            low = middle + 1u;
        } else {
            high = middle;
        }
    }
    if (low >= catalog->skeleton_count ||
        catalog->skeleton_piece_kind[low] != piece ||
        catalog->skeleton_cell_mask[low] != canonical_cell_ownership) {
        return false;
    }
    *out_skeleton_id = low;
    return true;
}
