#include "geometry_parent_hall_bound.h"

#include "../cache/cache_identity.h"

#include <string.h>

#define CLEARRA_HALL_PROOF_VERSION UINT64_C(1)
#define CLEARRA_COMPACT_PIECE_SUBSET_COUNT 128u
#define CLEARRA_ALL_STANDARD_PIECES_COMPACT UINT8_C(0x7f)

static uint8_t lowest_bit_index(uint64_t bit) {
    uint8_t index = 0u;
    while ((bit & UINT64_C(1)) == 0u) {
        bit >>= 1u;
        index++;
    }
    return index;
}

static uint8_t compact_piece_mask(uint64_t piece_mask) {
    uint8_t compact = 0u;
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        if ((piece_mask & (UINT64_C(1) << piece)) != 0u) {
            compact |= (uint8_t)(UINT8_C(1) << (piece - CLR_PIECE_I));
        }
    }
    return compact;
}

static bool active_member(
    const ClearraActivePieceFamily *active_family,
    uint16_t member_index) {
    uint16_t word = (uint16_t)(member_index / 64u);
    return (active_family->words[word] &
            (UINT64_C(1) << (member_index % 64u))) != 0u;
}

static void update_subset_maxima(
    const uint8_t remaining_counts[7],
    uint8_t maximum_by_subset[CLEARRA_COMPACT_PIECE_SUBSET_COUNT]) {
    uint8_t sums[CLEARRA_COMPACT_PIECE_SUBSET_COUNT] = {0u};
    for (uint8_t subset = 1u;
         subset < CLEARRA_COMPACT_PIECE_SUBSET_COUNT;
         ++subset) {
        uint8_t lowest = (uint8_t)(subset & (uint8_t)(0u - subset));
        uint8_t piece_index = 0u;
        uint8_t cursor = lowest;
        while ((cursor & UINT8_C(1)) == 0u) {
            cursor >>= 1u;
            piece_index++;
        }
        sums[subset] = (uint8_t)(
            sums[subset ^ lowest] + remaining_counts[piece_index]);
        if (sums[subset] > maximum_by_subset[subset]) {
            maximum_by_subset[subset] = sums[subset];
        }
    }
}

static bool compile_supply_subset_maxima(
    const ClearraGeometryExactCoverSearch *search,
    const ClearraActivePieceFamily *active_family,
    uint8_t maximum_by_subset[CLEARRA_COMPACT_PIECE_SUBSET_COUNT],
    uint8_t *out_active_member_count) {
    *out_active_member_count = 0u;
    if (search->piece_family_domain.constrained == 0u) {
        uint8_t remaining_counts[7] = {0u};
        for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
            uint8_t maximum =
                search->problem->piece_multiset_window.counts[piece];
            if (maximum < search->used_piece_counts[piece]) {
                return false;
            }
            remaining_counts[piece - CLR_PIECE_I] =
                (uint8_t)(maximum - search->used_piece_counts[piece]);
        }
        update_subset_maxima(remaining_counts, maximum_by_subset);
        *out_active_member_count = 1u;
        return true;
    }

    const clr_piece_multiset_family *family =
        &search->problem->piece_multiset_family;
    for (uint16_t member_index = 0u;
         member_index < family->count;
         ++member_index) {
        if (!active_member(active_family, member_index)) {
            continue;
        }
        uint8_t remaining_counts[7] = {0u};
        for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
            uint8_t maximum = family->members[member_index].counts[piece];
            if (maximum < search->used_piece_counts[piece]) {
                return false;
            }
            remaining_counts[piece - CLR_PIECE_I] =
                (uint8_t)(maximum - search->used_piece_counts[piece]);
        }
        update_subset_maxima(remaining_counts, maximum_by_subset);
        if (*out_active_member_count != UINT8_MAX) {
            (*out_active_member_count)++;
        }
    }
    return *out_active_member_count != 0u;
}

ClearraGeometryHallStatus clearra_geometry_parent_hall_bound_propagate(
    const ClearraGeometryExactCoverSearch *search,
    const ClearraActivePieceFamily *active_family,
    const ClearraGeometryDomainPropagation *domain,
    uint64_t remaining_cells,
    ClearraGeometryHallResult *out_result) {
    if (search == 0 || search->catalog == 0 || active_family == 0 ||
        domain == 0 || out_result == 0 || remaining_cells == 0u) {
        return CLEARRA_GEOMETRY_HALL_INVALID;
    }
    uint8_t cell_piece_masks[64] = {0u};
    bool has_restricted_cell = false;
    uint64_t cells = remaining_cells;
    while (cells != 0u) {
        uint64_t bit = cells & (~cells + UINT64_C(1));
        uint8_t cell = lowest_bit_index(bit);
        uint8_t allowed = compact_piece_mask(domain->cell_piece_masks[cell]);
        if (allowed == 0u) {
            return CLEARRA_GEOMETRY_HALL_INVALID;
        }
        cell_piece_masks[cell] = allowed;
        has_restricted_cell |= allowed != CLEARRA_ALL_STANDARD_PIECES_COMPACT;
        cells &= ~bit;
    }
    if (!has_restricted_cell) {
        *out_result = (ClearraGeometryHallResult){0};
        return CLEARRA_GEOMETRY_HALL_SKIPPED;
    }

    uint8_t maximum_by_subset[CLEARRA_COMPACT_PIECE_SUBSET_COUNT] = {0u};
    ClearraGeometryHallResult result = {
        .evidence_digest = clearra_cache_key_mix_u64(
            clearra_cache_key_mix_u64(
                UINT64_C(1469598103934665603),
                CLEARRA_HALL_PROOF_VERSION),
            remaining_cells),
    };
    if (!compile_supply_subset_maxima(
            search,
            active_family,
            maximum_by_subset,
            &result.active_family_member_count)) {
        return CLEARRA_GEOMETRY_HALL_INVALID;
    }

    for (uint8_t subset = 1u;
         subset < CLEARRA_COMPACT_PIECE_SUBSET_COUNT;
         ++subset) {
        uint8_t constrained_cells = 0u;
        cells = remaining_cells;
        while (cells != 0u) {
            uint64_t bit = cells & (~cells + UINT64_C(1));
            uint8_t cell = lowest_bit_index(bit);
            uint8_t allowed = cell_piece_masks[cell];
            if ((allowed & (uint8_t)~subset) == 0u) {
                constrained_cells++;
            }
            cells &= ~bit;
        }
        if (constrained_cells == 0u) {
            continue;
        }
        result.evidence_digest = clearra_cache_key_mix_u64(
            result.evidence_digest,
            ((uint64_t)subset << 32u) |
                ((uint64_t)constrained_cells << 16u) |
                maximum_by_subset[subset]);
        if (constrained_cells >
            (uint8_t)(maximum_by_subset[subset] * CLEARRA_TETROMINO_AREA)) {
            result.compact_piece_subset = subset;
            result.constrained_cell_count = constrained_cells;
            result.maximum_piece_count = maximum_by_subset[subset];
            if (result.evidence_digest == 0u) {
                result.evidence_digest = UINT64_C(1);
            }
            *out_result = result;
            return CLEARRA_GEOMETRY_HALL_IMPOSSIBLE;
        }
    }
    if (result.evidence_digest == 0u) {
        result.evidence_digest = UINT64_C(1);
    }
    *out_result = result;
    return CLEARRA_GEOMETRY_HALL_SUPPORTED;
}
