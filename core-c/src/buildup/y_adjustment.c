#include "buildup_internal.h"

#if defined(_MSC_VER) && defined(_M_X64)
#include <intrin.h>
#endif

static uint8_t count_bits16(uint16_t value) {
#if defined(_MSC_VER) && defined(_M_X64)
    return (uint8_t)__popcnt16(value);
#elif defined(__GNUC__) || defined(__clang__)
    return (uint8_t)__builtin_popcount((unsigned int)value);
#else
    uint8_t count = 0;
    while (value != 0u) {
        value = (uint16_t)(value & (uint16_t)(value - 1u));
        count++;
    }
    return count;
#endif
}

static uint8_t count_deleted_rows_below(uint16_t deleted_row_mask, uint8_t row) {
    if (row >= 16u) {
        return count_bits16(deleted_row_mask);
    }
    uint16_t lower_rows = row == 0u
                              ? 0u
                              : (uint16_t)((UINT16_C(1) << row) - 1u);
    return count_bits16((uint16_t)(deleted_row_mask & lower_rows));
}

static uint8_t trailing_zero_count64(uint64_t value) {
#if defined(_MSC_VER) && defined(_M_X64)
    unsigned long index = 0;
    (void)_BitScanForward64(&index, value);
    return (uint8_t)index;
#elif defined(__GNUC__) || defined(__clang__)
    return (uint8_t)__builtin_ctzll(value);
#else
    uint8_t count = 0u;
    while ((value & UINT64_C(1)) == 0u) {
        value >>= 1u;
        count++;
    }
    return count;
#endif
}

static bool line_clear_state_is_representable(ClearraLineClearState state) {
    return state.deleted_count == count_bits16(state.deleted_row_mask);
}

bool clearra_buildup_operation_matches_clear_state(
    const ClearraBuildUpState *state,
    const clr_buildup_operation *operation) {
    return state != 0 && operation != 0 &&
           operation->required_deleted_row_mask ==
               state->line_clear_state.deleted_row_mask;
}

bool clearra_buildup_operation_domain_may_match_clear_state(
    const clr_buildup_problem *problem,
    const ClearraBuildUpState *state,
    uint16_t operation_index) {
    if (problem == 0 || state == 0 ||
        operation_index >= problem->operation_set.operation_count ||
        operation_index >= CLR_BUILDUP_MAX_OPERATIONS) {
        return false;
    }

    uint16_t operation_bit =
        (uint16_t)(UINT16_C(1) << operation_index);
    if ((problem->operation_set.geometry_variant_domains & operation_bit) != 0u) {
        /* The representative is metadata; the current clear-state variant is
         * generated exactly before the operation is attempted. */
        return true;
    }
    return clearra_buildup_operation_matches_clear_state(
        state, &problem->operation_set.operations[operation_index]);
}

clr_buildup_status clearra_buildup_adjust_operation_for_line_clears(
    ClearraBoard64Layout layout,
    ClearraBuildUpState state,
    const clr_buildup_operation *operation,
    uint64_t *out_mask,
    int8_t *out_y) {
    if (!clearra_board64_layout_is_valid(layout) || operation == 0 || out_mask == 0 ||
        out_y == 0) {
        return CLR_BUILDUP_INVALID_ARGUMENT;
    }
    if ((operation->mask & ~layout.all_cells_mask) != 0) {
        return CLR_BUILDUP_Y_ADJUSTMENT_IMPOSSIBLE;
    }

    *out_mask = operation->mask;
    *out_y = operation->y;
    if (state.cleared_lines != state.line_clear_state.deleted_count) {
        return CLR_BUILDUP_Y_ADJUSTMENT_IMPOSSIBLE;
    }
    if (!line_clear_state_is_representable(state.line_clear_state)) {
        return CLR_BUILDUP_Y_ADJUSTMENT_IMPOSSIBLE;
    }
    if (!clearra_buildup_operation_matches_clear_state(&state, operation)) {
        return CLR_BUILDUP_Y_ADJUSTMENT_IMPOSSIBLE;
    }
    if (operation->y < 0) {
        return CLR_BUILDUP_Y_ADJUSTMENT_IMPOSSIBLE;
    }
    if (state.line_clear_state.deleted_row_mask == 0u) {
        uint64_t expected_mask = 0u;
        ClearraCandidateStatus candidate_status = clearra_candidate_mask_for_piece(
            layout,
            operation->piece,
            operation->rotation,
            operation->x,
            operation->y,
            &expected_mask);
        return candidate_status == CLEARRA_CANDIDATE_OK &&
                       expected_mask == operation->mask
                   ? CLR_BUILDUP_OK
                   : CLR_BUILDUP_Y_ADJUSTMENT_IMPOSSIBLE;
    }
    uint8_t operation_y = (uint8_t)operation->y;
    uint8_t anchor_deleted_below =
        count_deleted_rows_below(state.line_clear_state.deleted_row_mask, operation_y);
    if (anchor_deleted_below > operation_y) {
        return CLR_BUILDUP_Y_ADJUSTMENT_IMPOSSIBLE;
    }
    *out_y = (int8_t)(operation_y - anchor_deleted_below);

    uint64_t adjusted_mask = 0;
    uint64_t remaining_cells = operation->mask;
    while (remaining_cells != 0u) {
        uint8_t index = trailing_zero_count64(remaining_cells);
        remaining_cells &= remaining_cells - UINT64_C(1);

        uint8_t original_x = (uint8_t)(index % layout.width);
        uint8_t original_y = (uint8_t)(index / layout.width);
        uint16_t original_row_bit = original_y < 16u
            ? (uint16_t)(UINT16_C(1) << original_y)
            : 0u;
        if (original_row_bit == 0u ||
            (state.line_clear_state.deleted_row_mask & original_row_bit) != 0u) {
            return CLR_BUILDUP_Y_ADJUSTMENT_IMPOSSIBLE;
        }

        uint8_t deleted_below =
            count_deleted_rows_below(state.line_clear_state.deleted_row_mask, original_y);
        if (deleted_below > original_y) {
            return CLR_BUILDUP_Y_ADJUSTMENT_IMPOSSIBLE;
        }

        uint8_t adjusted_y = (uint8_t)(original_y - deleted_below);
        uint64_t adjusted_cell = 0;
        ClearraBoard64Status status =
            clearra_board64_cell_mask(layout, original_x, adjusted_y, &adjusted_cell);
        if (status != CLEARRA_BOARD64_OK || (adjusted_mask & adjusted_cell) != 0u) {
            return CLR_BUILDUP_Y_ADJUSTMENT_IMPOSSIBLE;
        }
        adjusted_mask |= adjusted_cell;
    }

    uint64_t expected_mask = 0;
    ClearraCandidateStatus candidate_status = clearra_candidate_mask_for_piece(
        layout, operation->piece, operation->rotation, operation->x, *out_y,
        &expected_mask);
    if (candidate_status != CLEARRA_CANDIDATE_OK || expected_mask != adjusted_mask) {
        return CLR_BUILDUP_Y_ADJUSTMENT_IMPOSSIBLE;
    }

    *out_mask = adjusted_mask;
    return (*out_mask & ~layout.all_cells_mask) == 0u
        ? CLR_BUILDUP_OK
        : CLR_BUILDUP_Y_ADJUSTMENT_IMPOSSIBLE;
}
