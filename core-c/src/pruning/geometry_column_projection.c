#include "geometry_column_projection.h"

#include "../cache/cache_identity.h"

#include <limits.h>
#include <string.h>

#define CLEARRA_COLUMN_PROJECTION_PROOF_VERSION UINT64_C(1)

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

static uint8_t lowest_allowed_count(uint16_t count_mask) {
    uint8_t count = 0u;
    while (count < 16u &&
           (count_mask & (uint16_t)(UINT16_C(1) << count)) == 0u) {
        count++;
    }
    return count;
}

static uint8_t highest_allowed_count(uint16_t count_mask) {
    uint8_t count = 15u;
    while (count != 0u &&
           (count_mask & (uint16_t)(UINT16_C(1) << count)) == 0u) {
        count--;
    }
    return count;
}

static uint64_t initial_evidence_digest(
    const ClearraGeometryExactCoverSearch *search,
    const ClearraActivePieceFamily *active_family,
    uint64_t remaining_cells) {
    uint64_t digest = UINT64_C(1469598103934665603);
    digest = clearra_cache_key_mix_u64(
        digest, CLEARRA_COLUMN_PROJECTION_PROOF_VERSION);
    digest = clearra_cache_key_mix_u64(
        digest, search->catalog->identity.board_layout_id);
    digest = clearra_cache_key_mix_u64(
        digest, search->catalog->identity.target_geometry_digest);
    digest = clearra_cache_key_mix_u64(
        digest, search->catalog->identity.support_table_digest);
    digest = clearra_cache_key_mix_u64(digest, remaining_cells);
    for (uint16_t word = 0u;
         word < CLEARRA_PIECE_FAMILY_DOMAIN_WORD_COUNT;
         ++word) {
        digest = clearra_cache_key_mix_u64(
            digest, active_family->words[word]);
    }
    return digest;
}

static void row_column_counts(
    ClearraBoard64Layout layout,
    uint64_t row_cells,
    uint8_t out_counts[16]) {
    memset(out_counts, 0, 16u * sizeof(*out_counts));
    while (row_cells != 0u) {
        uint64_t bit = row_cells & (~row_cells + UINT64_C(1));
        uint8_t cell = lowest_bit_index(bit);
        uint8_t x = (uint8_t)(cell % layout.width);
        out_counts[x]++;
        row_cells &= ~bit;
    }
}

ClearraGeometryColumnProjectionStatus
clearra_geometry_column_projection_propagate(
    const ClearraGeometryExactCoverSearch *search,
    const ClearraActivePieceFamily *active_family,
    uint64_t remaining_cells,
    uint8_t remaining_piece_count,
    ClearraGeometryColumnProjectionResult *out_result) {
    if (search == 0 || search->catalog == 0 || active_family == 0 ||
        out_result == 0 || remaining_cells == 0u ||
        remaining_piece_count == 0u ||
        remaining_piece_count > CLEARRA_PACKING_MAX_PIECES) {
        return CLEARRA_GEOMETRY_COLUMN_PROJECTION_INVALID;
    }
    ClearraBoard64Layout layout = search->catalog->layout;
    if (!clearra_board64_layout_is_valid(layout)) {
        return CLEARRA_GEOMETRY_COLUMN_PROJECTION_INVALID;
    }
    if (layout.width > 16u) {
        *out_result = (ClearraGeometryColumnProjectionResult){0};
        return CLEARRA_GEOMETRY_COLUMN_PROJECTION_SKIPPED;
    }

    ClearraGeometryColumnProjectionResult result = {
        .evidence_digest = initial_evidence_digest(
            search, active_family, remaining_cells),
        .failed_column = UINT8_MAX,
    };
    if (popcount64(remaining_cells) !=
        (uint8_t)(remaining_piece_count * CLEARRA_TETROMINO_AREA)) {
        result.failed_column = 0u;
        result.evidence_digest = clearra_cache_key_mix_u64(
            result.evidence_digest, UINT64_C(0x434f4c41524541));
        *out_result = result;
        return CLEARRA_GEOMETRY_COLUMN_PROJECTION_IMPOSSIBLE;
    }

    uint8_t demand[16] = {0u};
    uint64_t cells = remaining_cells;
    while (cells != 0u) {
        uint64_t bit = cells & (~cells + UINT64_C(1));
        uint8_t cell = lowest_bit_index(bit);
        demand[cell % layout.width]++;
        cells &= ~bit;
    }

    uint8_t piece_min[CLR_STANDARD_PIECE_KIND_COUNT][16];
    uint8_t piece_max[CLR_STANDARD_PIECE_KIND_COUNT][16] = {{0u}};
    memset(piece_min, UINT8_MAX, sizeof(piece_min));
    for (uint32_t row_id = 0u;
         row_id < search->catalog->skeleton_count;
         ++row_id) {
        ClearraActivePieceFamily ignored;
        if (!clearra_geometry_row_is_feasible(
                search,
                active_family,
                row_id,
                remaining_cells,
                &ignored)) {
            continue;
        }
        uint8_t piece =
            (uint8_t)search->catalog->skeleton_piece_kind[row_id];
        uint8_t counts[16];
        row_column_counts(
            layout, search->catalog->skeleton_cell_mask[row_id], counts);
        for (uint8_t x = 0u; x < layout.width; ++x) {
            if (counts[x] < piece_min[piece][x]) {
                piece_min[piece][x] = counts[x];
            }
            if (counts[x] > piece_max[piece][x]) {
                piece_max[piece][x] = counts[x];
            }
        }
        result.feasible_row_count++;
    }

    for (uint8_t x = 0u; x < layout.width; ++x) {
        uint16_t relaxed_minimum = 0u;
        uint16_t relaxed_maximum = 0u;
        for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
            uint16_t count_mask =
                clearra_geometry_piece_family_remaining_count_mask(
                    &search->piece_family_domain,
                    active_family,
                    piece,
                    search->used_piece_counts[piece],
                    search->problem->piece_multiset_window.counts[piece]);
            if (count_mask == 0u) {
                return CLEARRA_GEOMETRY_COLUMN_PROJECTION_INVALID;
            }
            uint8_t minimum_count = lowest_allowed_count(count_mask);
            uint8_t maximum_count = highest_allowed_count(count_mask);
            uint8_t minimum_projection = piece_min[piece][x] == UINT8_MAX
                ? 0u
                : piece_min[piece][x];
            relaxed_minimum = (uint16_t)(
                relaxed_minimum + minimum_count * minimum_projection);
            relaxed_maximum = (uint16_t)(
                relaxed_maximum + maximum_count * piece_max[piece][x]);
            result.evidence_digest = clearra_cache_key_mix_u64(
                result.evidence_digest,
                ((uint64_t)piece << 56u) |
                    ((uint64_t)x << 48u) |
                    ((uint64_t)count_mask << 24u) |
                    ((uint64_t)minimum_projection << 8u) |
                    piece_max[piece][x]);
        }
        if (demand[x] < relaxed_minimum || demand[x] > relaxed_maximum) {
            result.failed_column = x;
            result.demand = demand[x];
            result.relaxed_minimum = relaxed_minimum > UINT8_MAX
                ? UINT8_MAX
                : (uint8_t)relaxed_minimum;
            result.relaxed_maximum = relaxed_maximum > UINT8_MAX
                ? UINT8_MAX
                : (uint8_t)relaxed_maximum;
            result.evidence_digest = clearra_cache_key_mix_u64(
                result.evidence_digest,
                ((uint64_t)x << 48u) |
                    ((uint64_t)demand[x] << 32u) |
                    ((uint64_t)relaxed_minimum << 16u) |
                    relaxed_maximum);
            if (result.evidence_digest == 0u) {
                result.evidence_digest = UINT64_C(1);
            }
            *out_result = result;
            return CLEARRA_GEOMETRY_COLUMN_PROJECTION_IMPOSSIBLE;
        }
    }

    if (result.evidence_digest == 0u) {
        result.evidence_digest = UINT64_C(1);
    }
    *out_result = result;
    return CLEARRA_GEOMETRY_COLUMN_PROJECTION_SUPPORTED;
}
