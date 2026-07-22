#include "geometry_full_placement_domain.h"

#include <limits.h>

#define CLEARRA_FULL_PLACEMENT_DOMAIN_PROOF_VERSION UINT64_C(1)

static uint8_t single_bit_index(uint64_t bit) {
    uint8_t index = 0u;
    while ((bit & UINT64_C(1)) == 0u) {
        bit >>= 1u;
        index++;
    }
    return index;
}

static uint8_t popcount64(uint64_t value) {
    uint8_t count = 0u;
    while (value != 0u) {
        value &= value - UINT64_C(1);
        count++;
    }
    return count;
}

static uint64_t mix_digest(uint64_t digest, uint64_t value) {
    digest ^= value;
    digest *= UINT64_C(1099511628211);
    return digest;
}

static uint64_t domain_identity_digest(
    const ClearraGeometryExactCoverSearch *search,
    const ClearraActivePieceFamily *active_family,
    uint64_t remaining_cells) {
    const ClearraGeometryCatalogIdentity *identity =
        &search->catalog->identity;
    uint64_t digest = UINT64_C(1469598103934665603);
    digest = mix_digest(digest, identity->board_layout_id);
    digest = mix_digest(digest, identity->compact_universe_digest);
    digest = mix_digest(digest, identity->target_geometry_digest);
    digest = mix_digest(digest, identity->piece_catalog_id);
    digest = mix_digest(digest, identity->skeleton_projection_version);
    digest = mix_digest(digest, identity->rule_capability_id);
    digest = mix_digest(digest, identity->realization_table_digest);
    digest = mix_digest(digest, identity->support_table_digest);
    digest = mix_digest(
        digest, CLEARRA_FULL_PLACEMENT_DOMAIN_PROOF_VERSION);
    digest = mix_digest(digest, remaining_cells);
    for (uint16_t word = 0u;
         word < CLEARRA_PIECE_FAMILY_DOMAIN_WORD_COUNT;
         ++word) {
        digest = mix_digest(digest, active_family->words[word]);
    }
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        digest = mix_digest(
            digest,
            ((uint64_t)piece << 32u) | search->used_piece_counts[piece]);
    }
    return digest;
}

static uint8_t find_root(uint8_t parent[64], uint8_t cell) {
    uint8_t root = cell;
    while (parent[root] != root) {
        root = parent[root];
    }
    while (parent[cell] != cell) {
        uint8_t next = parent[cell];
        parent[cell] = root;
        cell = next;
    }
    return root;
}

static void union_cells(uint8_t parent[64], uint8_t left, uint8_t right) {
    uint8_t left_root = find_root(parent, left);
    uint8_t right_root = find_root(parent, right);
    if (left_root == right_root) {
        return;
    }
    if (right_root < left_root) {
        uint8_t swap = left_root;
        left_root = right_root;
        right_root = swap;
    }
    parent[right_root] = left_root;
}

static void union_common_owner_cells(
    uint8_t parent[64],
    uint8_t pivot,
    uint64_t common_cells) {
    while (common_cells != 0u) {
        uint64_t bit = common_cells & (~common_cells + UINT64_C(1));
        union_cells(parent, pivot, single_bit_index(bit));
        common_cells &= ~bit;
    }
}

static uint32_t exact_parent_rows_for_cells(
    const ClearraGeometryExactCoverSearch *search,
    const ClearraActivePieceFamily *active_family,
    uint64_t remaining_cells,
    uint64_t required_cells,
    uint64_t *out_piece_mask,
    uint64_t *out_digest) {
    uint64_t first_bit = required_cells & (~required_cells + UINT64_C(1));
    uint8_t first_cell = single_bit_index(first_bit);
    uint32_t begin = search->catalog->cell_support_offsets[first_cell];
    uint32_t end = search->catalog->cell_support_offsets[first_cell + 1u];
    uint32_t support_count = 0u;
    uint64_t piece_mask = 0u;
    uint64_t digest = UINT64_C(1469598103934665603);
    for (uint32_t cursor = begin; cursor < end; ++cursor) {
        uint32_t row_id = search->catalog->cell_support_row_ids[cursor];
        uint64_t row_cells = search->catalog->skeleton_cell_mask[row_id];
        ClearraActivePieceFamily ignored;
        if ((row_cells & required_cells) != required_cells ||
            !clearra_geometry_row_is_feasible(
                search,
                active_family,
                row_id,
                remaining_cells,
                &ignored)) {
            continue;
        }
        support_count++;
        piece_mask |= UINT64_C(1)
                      << search->catalog->skeleton_piece_kind[row_id];
        digest = mix_digest(digest, (uint64_t)row_id + 1u);
    }
    *out_piece_mask = piece_mask;
    *out_digest = digest;
    return support_count;
}

ClearraGeometryDomainStatus clearra_geometry_full_placement_domain_propagate(
    const ClearraGeometryExactCoverSearch *search,
    const ClearraActivePieceFamily *active_family,
    uint64_t remaining_cells,
    ClearraGeometryDomainPropagation *out_propagation) {
    if (search == 0 || search->catalog == 0 || active_family == 0 ||
        out_propagation == 0 || remaining_cells == 0u) {
        return CLEARRA_GEOMETRY_DOMAIN_INVALID;
    }

    ClearraGeometryDomainPropagation result = {
        .pivot_required_cells = 0u,
        .evidence_digest = domain_identity_digest(
            search, active_family, remaining_cells),
        .pivot_piece_mask = 0u,
        .pivot_support_count = UINT32_MAX,
        .pivot_cell = UINT8_MAX,
    };
    uint8_t same_tile_parent[64];
    uint64_t common_owner_cells[64];
    uint32_t support_counts[64];
    for (uint8_t cell = 0u; cell < 64u; ++cell) {
        same_tile_parent[cell] = cell;
    }

    bool has_nontrivial_same_tile_certificate = false;
    uint64_t cells = remaining_cells;
    while (cells != 0u) {
        uint64_t bit = cells & (~cells + UINT64_C(1));
        uint8_t cell = single_bit_index(bit);
        uint32_t begin = search->catalog->cell_support_offsets[cell];
        uint32_t end = search->catalog->cell_support_offsets[cell + 1u];
        uint32_t support_count = 0u;
        uint64_t piece_mask = 0u;
        uint64_t common_cells = remaining_cells;
        uint64_t cell_digest = UINT64_C(1469598103934665603);

        for (uint32_t cursor = begin; cursor < end; ++cursor) {
            uint32_t row_id = search->catalog->cell_support_row_ids[cursor];
            ClearraActivePieceFamily ignored;
            if (!clearra_geometry_row_is_feasible(
                    search,
                    active_family,
                    row_id,
                    remaining_cells,
                    &ignored)) {
                continue;
            }
            uint32_t piece = search->catalog->skeleton_piece_kind[row_id];
            support_count++;
            piece_mask |= UINT64_C(1) << piece;
            common_cells &= search->catalog->skeleton_cell_mask[row_id];
            cell_digest = mix_digest(cell_digest, (uint64_t)row_id + 1u);
        }

        result.evidence_digest = mix_digest(result.evidence_digest, cell);
        result.evidence_digest = mix_digest(result.evidence_digest, support_count);
        result.evidence_digest = mix_digest(result.evidence_digest, piece_mask);
        result.evidence_digest = mix_digest(result.evidence_digest, cell_digest);
        if (support_count == 0u) {
            result.pivot_cell = cell;
            result.pivot_required_cells = bit;
            result.pivot_support_count = 0u;
            result.pivot_piece_mask = 0u;
            *out_propagation = result;
            return CLEARRA_GEOMETRY_DOMAIN_EMPTY;
        }
        common_owner_cells[cell] = common_cells;
        support_counts[cell] = support_count;
        result.cell_piece_masks[cell] = (uint8_t)piece_mask;
        if (popcount64(common_cells) > 1u) {
            has_nontrivial_same_tile_certificate = true;
        }
        if (support_count < result.pivot_support_count) {
            result.pivot_cell = cell;
            result.pivot_required_cells = bit;
            result.pivot_support_count = support_count;
            result.pivot_piece_mask = piece_mask;
        }
        cells &= ~bit;
    }

    if (result.pivot_cell == UINT8_MAX) {
        return CLEARRA_GEOMETRY_DOMAIN_INVALID;
    }

    /*
     * A cell must be owned by one of its complete feasible parent rows. Cells
     * shared by every such row are owned by that same tile. Overlapping exact
     * certificates must also share one parent row, so their transitive union
     * may be checked against the complete full-placement domain. No visual
     * Arm/Elbow guess is promoted to pruning authority here.
     */
    if (has_nontrivial_same_tile_certificate) {
        cells = remaining_cells;
        while (cells != 0u) {
            uint64_t bit = cells & (~cells + UINT64_C(1));
            uint8_t cell = single_bit_index(bit);
            union_common_owner_cells(
                same_tile_parent, cell, common_owner_cells[cell]);
            cells &= ~bit;
        }

        uint64_t group_masks[64] = {0};
        cells = remaining_cells;
        while (cells != 0u) {
            uint64_t bit = cells & (~cells + UINT64_C(1));
            uint8_t cell = single_bit_index(bit);
            group_masks[find_root(same_tile_parent, cell)] |= bit;
            cells &= ~bit;
        }

        for (uint8_t root = 0u; root < 64u; ++root) {
            uint64_t required_cells = group_masks[root];
            uint8_t required_cell_count = popcount64(required_cells);
            if (required_cell_count <= 1u) {
                continue;
            }
            result.same_tile_certificate_count++;
            result.evidence_digest = mix_digest(
                result.evidence_digest, required_cells);
            if (required_cell_count > CLEARRA_TETROMINO_AREA) {
                result.pivot_cell = root;
                result.pivot_required_cells = required_cells;
                result.pivot_support_count = 0u;
                result.pivot_piece_mask = 0u;
                result.pivot_filtered_row_count = support_counts[root];
                *out_propagation = result;
                return CLEARRA_GEOMETRY_DOMAIN_EMPTY;
            }

            uint64_t parent_piece_mask = 0u;
            uint64_t parent_digest = 0u;
            uint32_t parent_count = exact_parent_rows_for_cells(
                search,
                active_family,
                remaining_cells,
                required_cells,
                &parent_piece_mask,
                &parent_digest);
            result.evidence_digest = mix_digest(
                result.evidence_digest, parent_digest);
            if (parent_count == 0u) {
                result.pivot_cell = root;
                result.pivot_required_cells = required_cells;
                result.pivot_support_count = 0u;
                result.pivot_piece_mask = 0u;
                result.pivot_filtered_row_count = support_counts[root];
                *out_propagation = result;
                return CLEARRA_GEOMETRY_DOMAIN_EMPTY;
            }
            if (parent_count < result.pivot_support_count) {
                uint8_t pivot_cell = single_bit_index(
                    required_cells & (~required_cells + UINT64_C(1)));
                result.pivot_cell = pivot_cell;
                result.pivot_required_cells = required_cells;
                result.pivot_support_count = parent_count;
                result.pivot_filtered_row_count =
                    support_counts[pivot_cell] - parent_count;
                result.pivot_piece_mask = parent_piece_mask;
            }
        }
    }

    if (result.evidence_digest == 0u) {
        result.evidence_digest = UINT64_C(1);
    }
    *out_propagation = result;
    return CLEARRA_GEOMETRY_DOMAIN_SUPPORTED;
}
