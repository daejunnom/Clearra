#include "geometry_bumper_domain.h"

#include "../cache/cache_identity.h"

#define CLEARRA_BUMPER_DOMAIN_PROOF_VERSION UINT64_C(1)

static uint8_t popcount64(uint64_t value) {
    uint8_t count = 0u;
    while (value != 0u) {
        value &= value - UINT64_C(1);
        count++;
    }
    return count;
}

static uint64_t columns_before_mask(
    ClearraBoard64Layout layout,
    uint8_t bumper_column) {
    uint64_t mask = 0u;
    for (uint8_t row = 0u; row < layout.height; ++row) {
        for (uint8_t column = 0u; column < bumper_column; ++column) {
            mask |= UINT64_C(1) << (row * layout.width + column);
        }
    }
    return mask;
}

static uint64_t columns_after_mask(
    ClearraBoard64Layout layout,
    uint8_t bumper_column) {
    uint64_t mask = 0u;
    for (uint8_t row = 0u; row < layout.height; ++row) {
        for (uint8_t column = (uint8_t)(bumper_column + 1u);
             column < layout.width;
             ++column) {
            mask |= UINT64_C(1) << (row * layout.width + column);
        }
    }
    return mask;
}

static bool is_bumper_column(
    const ClearraGeometryCatalog *catalog,
    uint64_t remaining_cells,
    uint8_t column,
    uint8_t *out_cell) {
    if (catalog->layout.height < 2u) {
        return false;
    }
    uint64_t lower = 0u;
    for (uint8_t row = 0u; row + 1u < catalog->layout.height; ++row) {
        lower |= UINT64_C(1) << (row * catalog->layout.width + column);
    }
    uint8_t top_cell = (uint8_t)(
        (catalog->layout.height - 1u) * catalog->layout.width + column);
    uint64_t top = UINT64_C(1) << top_cell;
    if ((catalog->initial_board & lower) != lower ||
        (catalog->initial_board & top) != 0u ||
        (remaining_cells & top) == 0u ||
        (remaining_cells & lower) != 0u) {
        return false;
    }
    *out_cell = top_cell;
    return true;
}

bool clearra_geometry_bumper_row_is_compatible(
    const ClearraGeometryCatalog *catalog,
    uint64_t remaining_cells,
    uint8_t bumper_cell,
    uint32_t row_id) {
    if (catalog == 0 || bumper_cell >= catalog->layout.cell_count ||
        row_id >= catalog->skeleton_count) {
        return false;
    }
    uint8_t bumper_column = (uint8_t)(
        bumper_cell % catalog->layout.width);
    uint64_t bumper_bit = UINT64_C(1) << bumper_cell;
    uint64_t row = catalog->skeleton_cell_mask[row_id];
    if ((row & bumper_bit) == 0u || (row & remaining_cells) != row) {
        return false;
    }
    uint64_t left_mask = columns_before_mask(
        catalog->layout, bumper_column);
    uint64_t right_mask = columns_after_mask(
        catalog->layout, bumper_column);
    uint8_t left_demand = popcount64(remaining_cells & left_mask);
    uint8_t right_demand = popcount64(remaining_cells & right_mask);
    uint8_t left_supply = popcount64(row & left_mask);
    uint8_t right_supply = popcount64(row & right_mask);
    return left_supply <= left_demand && right_supply <= right_demand &&
           (uint8_t)(left_demand - left_supply) % 4u == 0u &&
           (uint8_t)(right_demand - right_supply) % 4u == 0u;
}

ClearraGeometryBumperStatus clearra_geometry_bumper_domain_propagate(
    const ClearraGeometryExactCoverSearch *search,
    const ClearraActivePieceFamily *active_family,
    uint64_t remaining_cells,
    ClearraGeometryBumperResult *out_result) {
    if (search == 0 || search->catalog == 0 || active_family == 0 ||
        out_result == 0 || remaining_cells == 0u) {
        return CLEARRA_GEOMETRY_BUMPER_INVALID;
    }
    bool found = false;
    ClearraGeometryBumperResult best = {0};
    best.exact_parent_row_count = UINT32_MAX;
    for (uint8_t column = 0u;
         column < search->catalog->layout.width;
         ++column) {
        uint8_t bumper_cell = 0u;
        if (!is_bumper_column(
                search->catalog, remaining_cells, column, &bumper_cell)) {
            continue;
        }
        ClearraGeometryBumperResult candidate = {
            .evidence_digest = clearra_cache_key_mix_u64(
                clearra_cache_key_mix_u64(
                    search->catalog->identity.support_table_digest,
                    CLEARRA_BUMPER_DOMAIN_PROOF_VERSION),
                remaining_cells),
            .bumper_cell = bumper_cell,
            .bumper_column = column,
        };
        uint32_t begin =
            search->catalog->cell_support_offsets[bumper_cell];
        uint32_t end =
            search->catalog->cell_support_offsets[bumper_cell + 1u];
        uint64_t left_mask = columns_before_mask(
            search->catalog->layout, column);
        uint64_t right_mask = columns_after_mask(
            search->catalog->layout, column);
        for (uint32_t cursor = begin; cursor < end; ++cursor) {
            uint32_t row_id =
                search->catalog->cell_support_row_ids[cursor];
            ClearraActivePieceFamily ignored;
            if (!clearra_geometry_row_is_feasible(
                    search,
                    active_family,
                    row_id,
                    remaining_cells,
                    &ignored)) {
                continue;
            }
            if (!clearra_geometry_bumper_row_is_compatible(
                    search->catalog,
                    remaining_cells,
                    bumper_cell,
                    row_id)) {
                candidate.filtered_parent_row_count++;
                continue;
            }
            uint64_t row = search->catalog->skeleton_cell_mask[row_id];
            uint8_t left = popcount64(row & left_mask);
            uint8_t right = popcount64(row & right_mask);
            if (left == 0u || right == 0u) {
                candidate.outer_three_row_count++;
            } else {
                candidate.split_two_one_row_count++;
            }
            candidate.exact_parent_row_count++;
            candidate.parent_piece_mask |=
                UINT64_C(1)
                << search->catalog->skeleton_piece_kind[row_id];
            candidate.evidence_digest = clearra_cache_key_mix_u64(
                candidate.evidence_digest, (uint64_t)row_id + 1u);
        }
        candidate.evidence_digest = clearra_cache_key_mix_u64(
            candidate.evidence_digest,
            ((uint64_t)candidate.exact_parent_row_count << 32u) |
                candidate.filtered_parent_row_count);
        if (candidate.evidence_digest == 0u) {
            candidate.evidence_digest = UINT64_C(1);
        }
        if (!found || candidate.exact_parent_row_count <
                          best.exact_parent_row_count ||
            (candidate.exact_parent_row_count ==
                 best.exact_parent_row_count &&
             candidate.bumper_cell < best.bumper_cell)) {
            best = candidate;
            found = true;
        }
    }
    if (!found) {
        *out_result = (ClearraGeometryBumperResult){0};
        return CLEARRA_GEOMETRY_BUMPER_SKIPPED;
    }
    *out_result = best;
    return best.exact_parent_row_count == 0u
        ? CLEARRA_GEOMETRY_BUMPER_EMPTY
        : CLEARRA_GEOMETRY_BUMPER_SUPPORTED;
}
