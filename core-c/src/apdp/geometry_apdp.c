#include "geometry_apdp.h"

#include "../cache/cache_identity.h"

#define CLEARRA_APDP_PROOF_VERSION UINT64_C(1)
#define CLEARRA_APDP_SHAPE_ARM UINT8_C(1)
#define CLEARRA_APDP_SHAPE_ELBOW UINT8_C(2)

static uint8_t popcount64(uint64_t value) {
    uint8_t count = 0u;
    while (value != 0u) {
        value &= value - UINT64_C(1);
        count++;
    }
    return count;
}

static uint8_t lowest_bit_index(uint64_t bit) {
    uint8_t index = 0u;
    while ((bit & UINT64_C(1)) == 0u) {
        bit >>= 1u;
        index++;
    }
    return index;
}

static uint8_t partial_shape_kind(
    ClearraBoard64Layout layout,
    uint64_t cells) {
    if (popcount64(cells) != 3u) {
        return 0u;
    }
    uint8_t min_x = UINT8_MAX;
    uint8_t max_x = 0u;
    uint8_t min_y = UINT8_MAX;
    uint8_t max_y = 0u;
    while (cells != 0u) {
        uint64_t bit = cells & (~cells + UINT64_C(1));
        uint8_t cell = lowest_bit_index(bit);
        uint8_t x = (uint8_t)(cell % layout.width);
        uint8_t y = (uint8_t)(cell / layout.width);
        min_x = x < min_x ? x : min_x;
        max_x = x > max_x ? x : max_x;
        min_y = y < min_y ? y : min_y;
        max_y = y > max_y ? y : max_y;
        cells &= ~bit;
    }
    if ((min_y == max_y && (uint8_t)(max_x - min_x) == 2u) ||
        (min_x == max_x && (uint8_t)(max_y - min_y) == 2u)) {
        return CLEARRA_APDP_SHAPE_ARM;
    }
    if ((uint8_t)(max_x - min_x) == 1u &&
        (uint8_t)(max_y - min_y) == 1u) {
        return CLEARRA_APDP_SHAPE_ELBOW;
    }
    return 0u;
}

static uint8_t pair_flag(uint8_t left_kind, uint8_t right_kind) {
    if (left_kind == CLEARRA_APDP_SHAPE_ARM &&
        right_kind == CLEARRA_APDP_SHAPE_ARM) {
        return CLEARRA_APDP_SUPPORT_ARM_ARM;
    }
    if (left_kind == CLEARRA_APDP_SHAPE_ELBOW &&
        right_kind == CLEARRA_APDP_SHAPE_ELBOW) {
        return CLEARRA_APDP_SUPPORT_ELBOW_ELBOW;
    }
    return CLEARRA_APDP_SUPPORT_ARM_ELBOW;
}

static uint8_t support_flags_for_row(
    ClearraBoard64Layout layout,
    uint64_t row_cells) {
    if (popcount64(row_cells) != CLEARRA_TETROMINO_AREA) {
        return 0u;
    }
    uint64_t partials[4] = {0u};
    uint8_t kinds[4] = {0u};
    uint8_t count = 0u;
    uint64_t cells = row_cells;
    while (cells != 0u) {
        uint64_t bit = cells & (~cells + UINT64_C(1));
        partials[count] = row_cells & ~bit;
        kinds[count] = partial_shape_kind(layout, partials[count]);
        count++;
        cells &= ~bit;
    }
    uint8_t flags = 0u;
    for (uint8_t left = 0u; left < count; ++left) {
        if (kinds[left] == 0u) {
            continue;
        }
        for (uint8_t right = (uint8_t)(left + 1u);
             right < count;
             ++right) {
            if (kinds[right] == 0u ||
                (partials[left] | partials[right]) != row_cells) {
                continue;
            }
            flags |= pair_flag(kinds[left], kinds[right]);
        }
    }
    return flags;
}

bool clearra_geometry_apdp_compile_support_flags(
    ClearraBoard64Layout layout,
    const uint64_t *skeleton_cell_masks,
    uint32_t skeleton_count,
    uint8_t *out_support_flags) {
    if (!clearra_board64_layout_is_valid(layout) ||
        (skeleton_count != 0u &&
         (skeleton_cell_masks == 0 || out_support_flags == 0))) {
        return false;
    }
    for (uint32_t row_id = 0u; row_id < skeleton_count; ++row_id) {
        uint8_t flags = support_flags_for_row(
            layout, skeleton_cell_masks[row_id]);
        if (flags == 0u) {
            return false;
        }
        out_support_flags[row_id] = flags;
    }
    return true;
}

static bool row_has_exact_partial_pair(
    ClearraBoard64Layout layout,
    uint64_t row_cells,
    uint64_t required_cells) {
    uint8_t required_kind = partial_shape_kind(layout, required_cells);
    if (required_kind == 0u ||
        (row_cells & required_cells) != required_cells) {
        return false;
    }
    uint64_t cells = row_cells;
    while (cells != 0u) {
        uint64_t bit = cells & (~cells + UINT64_C(1));
        uint64_t other = row_cells & ~bit;
        if (partial_shape_kind(layout, other) != 0u &&
            (required_cells | other) == row_cells) {
            return true;
        }
        cells &= ~bit;
    }
    return false;
}

bool clearra_geometry_apdp_row_supports_required_cells(
    const ClearraGeometryCatalog *catalog,
    uint32_t row_id,
    uint64_t required_same_tile_cells) {
    if (catalog == 0 || row_id >= catalog->skeleton_count ||
        catalog->skeleton_apdp_support_flags == 0) {
        return false;
    }
    uint8_t required_kind = partial_shape_kind(
        catalog->layout, required_same_tile_cells);
    uint8_t flags = catalog->skeleton_apdp_support_flags[row_id];
    bool class_can_support = required_kind == CLEARRA_APDP_SHAPE_ARM
        ? (flags & (CLEARRA_APDP_SUPPORT_ARM_ARM |
                    CLEARRA_APDP_SUPPORT_ARM_ELBOW)) != 0u
        : required_kind == CLEARRA_APDP_SHAPE_ELBOW &&
              (flags & (CLEARRA_APDP_SUPPORT_ARM_ELBOW |
                        CLEARRA_APDP_SUPPORT_ELBOW_ELBOW)) != 0u;
    return class_can_support && row_has_exact_partial_pair(
        catalog->layout,
        catalog->skeleton_cell_mask[row_id],
        required_same_tile_cells);
}

ClearraGeometryApdpStatus clearra_geometry_apdp_propagate(
    const ClearraGeometryExactCoverSearch *search,
    const ClearraActivePieceFamily *active_family,
    uint64_t remaining_cells,
    uint64_t required_same_tile_cells,
    ClearraGeometryApdpResult *out_result) {
    if (search == 0 || search->catalog == 0 || active_family == 0 ||
        out_result == 0 || remaining_cells == 0u ||
        search->catalog->skeleton_apdp_support_flags == 0) {
        return CLEARRA_GEOMETRY_APDP_INVALID;
    }
    uint8_t required_kind = partial_shape_kind(
        search->catalog->layout, required_same_tile_cells);
    if (required_kind == 0u) {
        *out_result = (ClearraGeometryApdpResult){0};
        return CLEARRA_GEOMETRY_APDP_SKIPPED;
    }

    ClearraGeometryApdpResult result = {
        .exact_parent_row_digest = clearra_cache_key_mix_u64(
            clearra_cache_key_mix_u64(
                UINT64_C(1469598103934665603),
                CLEARRA_APDP_PROOF_VERSION),
            required_same_tile_cells),
        .partial_shape_kind = required_kind,
    };
    uint64_t first_bit = required_same_tile_cells &
                         (~required_same_tile_cells + UINT64_C(1));
    uint8_t first_cell = lowest_bit_index(first_bit);
    uint32_t begin = search->catalog->cell_support_offsets[first_cell];
    uint32_t end = search->catalog->cell_support_offsets[first_cell + 1u];
    for (uint32_t cursor = begin; cursor < end; ++cursor) {
        uint32_t row_id = search->catalog->cell_support_row_ids[cursor];
        uint64_t row_cells = search->catalog->skeleton_cell_mask[row_id];
        ClearraActivePieceFamily ignored;
        if ((row_cells & required_same_tile_cells) !=
                required_same_tile_cells ||
            !clearra_geometry_row_is_feasible(
                search,
                active_family,
                row_id,
                remaining_cells,
                &ignored)) {
            continue;
        }
        if (!clearra_geometry_apdp_row_supports_required_cells(
                search->catalog, row_id, required_same_tile_cells)) {
            result.filtered_parent_row_count++;
            continue;
        }
        result.exact_parent_row_count++;
        result.parent_piece_mask |= UINT64_C(1)
                                    << search->catalog
                                           ->skeleton_piece_kind[row_id];
        result.exact_parent_row_digest = clearra_cache_key_mix_u64(
            result.exact_parent_row_digest, (uint64_t)row_id + UINT64_C(1));
    }
    result.exact_parent_row_digest = clearra_cache_key_mix_u64(
        result.exact_parent_row_digest,
        ((uint64_t)result.exact_parent_row_count << 32u) |
            result.filtered_parent_row_count);
    if (result.exact_parent_row_digest == 0u) {
        result.exact_parent_row_digest = UINT64_C(1);
    }
    *out_result = result;
    return result.exact_parent_row_count == 0u
        ? CLEARRA_GEOMETRY_APDP_EMPTY
        : CLEARRA_GEOMETRY_APDP_SUPPORTED;
}
