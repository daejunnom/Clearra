#include "geometry_additive_invariant.h"

#include "../cache/cache_identity.h"

#include <string.h>

#define CLEARRA_ADDITIVE_SIGNATURE_MODULUS 64u

static uint8_t cell_weight(uint8_t bank, uint8_t x, uint8_t y) {
    static const uint8_t column_weights[16] = {
        1u, 2u, 4u, 8u, 16u, 32u, 3u, 6u,
        12u, 24u, 48u, 33u, 5u, 10u, 20u, 40u};
    static const uint8_t four_color_weights[4] = {1u, 5u, 17u, 29u};
    if (bank == 0u) {
        return ((x + y) & 1u) == 0u ? 1u : 63u;
    }
    if (bank == 1u) {
        return column_weights[x & 15u];
    }
    return four_color_weights[(x & 1u) | ((y & 1u) << 1u)];
}

static uint8_t mask_signature(
    ClearraBoard64Layout layout,
    uint64_t cells,
    uint8_t bank) {
    uint8_t signature = 0u;
    while (cells != 0u) {
        uint64_t bit = cells & (~cells + UINT64_C(1));
        uint8_t index = 0u;
        uint64_t cursor = bit;
        while ((cursor & UINT64_C(1)) == 0u) {
            cursor >>= 1u;
            index++;
        }
        uint8_t x = (uint8_t)(index % layout.width);
        uint8_t y = (uint8_t)(index / layout.width);
        signature = (uint8_t)(
            (signature + cell_weight(bank, x, y)) &
            (CLEARRA_ADDITIVE_SIGNATURE_MODULUS - 1u));
        cells &= ~bit;
    }
    return signature;
}

bool clearra_geometry_additive_invariant_compile_signatures(
    ClearraBoard64Layout layout,
    const uint64_t *skeleton_cell_masks,
    uint32_t skeleton_count,
    uint8_t *out_signatures) {
    if (!clearra_board64_layout_is_valid(layout) ||
        (skeleton_count != 0u &&
         (skeleton_cell_masks == 0 || out_signatures == 0))) {
        return false;
    }
    for (uint32_t row_id = 0u; row_id < skeleton_count; ++row_id) {
        for (uint8_t bank = 0u;
             bank < CLEARRA_ADDITIVE_SIGNATURE_BANK_COUNT;
             ++bank) {
            out_signatures[
                (size_t)row_id * CLEARRA_ADDITIVE_SIGNATURE_BANK_COUNT +
                bank] = mask_signature(
                    layout, skeleton_cell_masks[row_id], bank);
        }
    }
    return true;
}

static uint64_t rotate_signature_set(uint64_t values, uint8_t amount) {
    amount &= 63u;
    return amount == 0u
        ? values
        : (values << amount) | (values >> (64u - amount));
}

static uint64_t signature_sumset(uint64_t left, uint64_t right) {
    uint64_t result = 0u;
    while (right != 0u) {
        uint64_t bit = right & (~right + UINT64_C(1));
        uint8_t shift = 0u;
        uint64_t cursor = bit;
        while ((cursor & UINT64_C(1)) == 0u) {
            cursor >>= 1u;
            shift++;
        }
        result |= rotate_signature_set(left, shift);
        right &= ~bit;
    }
    return result;
}

static bool bank_reaches_target(
    const ClearraGeometryExactCoverSearch *search,
    const ClearraActivePieceFamily *active_family,
    uint64_t remaining_cells,
    uint8_t remaining_piece_count,
    uint8_t bank,
    uint64_t *out_evidence_digest) {
    uint64_t row_signatures[CLR_STANDARD_PIECE_KIND_COUNT] = {0};
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
        uint8_t signature = search->catalog->skeleton_additive_signatures[
            (size_t)row_id * CLEARRA_ADDITIVE_SIGNATURE_BANK_COUNT + bank];
        row_signatures[piece] |= UINT64_C(1) << signature;
    }

    uint64_t reachable[CLEARRA_PACKING_MAX_PIECES + 1u] = {0};
    reachable[0] = UINT64_C(1);
    uint8_t processed_piece_count = 0u;
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        uint16_t allowed_counts =
            clearra_geometry_piece_family_remaining_count_mask(
                &search->piece_family_domain,
                active_family,
                piece,
                search->used_piece_counts[piece],
                search->problem->piece_multiset_window.counts[piece]);
        if (allowed_counts == 0u) {
            return false;
        }
        uint64_t repeated[CLEARRA_PACKING_MAX_PIECES + 1u] = {0};
        repeated[0] = UINT64_C(1);
        for (uint8_t count = 1u; count <= remaining_piece_count; ++count) {
            repeated[count] = signature_sumset(
                repeated[count - 1u], row_signatures[piece]);
        }
        uint64_t next[CLEARRA_PACKING_MAX_PIECES + 1u] = {0};
        for (uint8_t total = 0u;
             total <= processed_piece_count && total <= remaining_piece_count;
             ++total) {
            if (reachable[total] == 0u) {
                continue;
            }
            for (uint8_t count = 0u;
                 count + total <= remaining_piece_count;
                 ++count) {
                if ((allowed_counts & (uint16_t)(UINT16_C(1) << count)) == 0u ||
                    repeated[count] == 0u) {
                    continue;
                }
                next[total + count] |=
                    signature_sumset(reachable[total], repeated[count]);
            }
        }
        memcpy(reachable, next, sizeof(reachable));
        processed_piece_count = remaining_piece_count;
    }

    uint8_t target = mask_signature(
        search->catalog->layout, remaining_cells, bank);
    *out_evidence_digest = clearra_cache_key_mix_u64(
        clearra_cache_key_mix_u64(
            UINT64_C(1469598103934665603), remaining_cells),
        reachable[remaining_piece_count] ^ ((uint64_t)target << 56u));
    return (reachable[remaining_piece_count] & (UINT64_C(1) << target)) != 0u;
}

ClearraGeometryInvariantStatus clearra_geometry_additive_invariant_propagate(
    const ClearraGeometryExactCoverSearch *search,
    const ClearraActivePieceFamily *active_family,
    uint64_t remaining_cells,
    uint8_t remaining_piece_count,
    ClearraGeometryInvariantResult *out_result) {
    if (search == 0 || search->catalog == 0 || active_family == 0 ||
        out_result == 0 || remaining_cells == 0u ||
        remaining_piece_count == 0u ||
        remaining_piece_count > CLEARRA_PACKING_MAX_PIECES ||
        (search->catalog->skeleton_count != 0u &&
         search->catalog->skeleton_additive_signatures == 0)) {
        return CLEARRA_GEOMETRY_INVARIANT_INVALID;
    }
    ClearraGeometryInvariantResult result = {
        .evidence_digest = clearra_cache_key_mix_u64(
            UINT64_C(1469598103934665603),
            CLEARRA_ADDITIVE_INVARIANT_PROOF_VERSION),
        .failed_bank = UINT8_MAX,
        .checked_bank_count = 0u,
    };
    for (uint8_t bank = 0u;
         bank < CLEARRA_ADDITIVE_SIGNATURE_BANK_COUNT;
         ++bank) {
        uint64_t bank_digest = 0u;
        result.checked_bank_count++;
        if (!bank_reaches_target(
                search,
                active_family,
                remaining_cells,
                remaining_piece_count,
                bank,
                &bank_digest)) {
            result.failed_bank = bank;
            result.evidence_digest = clearra_cache_key_mix_u64(
                result.evidence_digest, bank_digest ^ bank);
            *out_result = result;
            return CLEARRA_GEOMETRY_INVARIANT_IMPOSSIBLE;
        }
        result.evidence_digest = clearra_cache_key_mix_u64(
            result.evidence_digest, bank_digest ^ bank);
    }
    if (result.evidence_digest == 0u) {
        result.evidence_digest = UINT64_C(1);
    }
    *out_result = result;
    return CLEARRA_GEOMETRY_INVARIANT_SUPPORTED;
}
