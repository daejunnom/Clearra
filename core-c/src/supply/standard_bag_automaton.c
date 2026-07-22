#include "standard_bag_automaton.h"

#include <limits.h>

static uint64_t piece_count_unit(uint8_t piece) {
    return UINT64_C(1) << ((uint64_t)piece * 4u);
}

static uint64_t standard_bag_storage_mask(void) {
    uint64_t mask = 0u;
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        mask |= UINT64_C(0xf) << ((uint64_t)piece * 4u);
    }
    return mask;
}

uint64_t clearra_standard_bag_full_remainder_key(void) {
    uint64_t key = 0u;
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        key += piece_count_unit(piece);
    }
    return key;
}

bool clearra_standard_bag_remainder_key_is_exact(uint64_t key) {
    if ((key & ~standard_bag_storage_mask()) != 0u) {
        return false;
    }
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        uint64_t count = (key >> ((uint64_t)piece * 4u)) & UINT64_C(0xf);
        if (count > 1u) {
            return false;
        }
    }
    return true;
}

static ClearraStandardBagAutomatonStatus refill_if_needed(
    const clr_hold_automaton_state *state,
    clr_hold_automaton_state *out_state) {
    if (state == 0 || out_state == 0 ||
        !clearra_standard_bag_remainder_key_is_exact(state->bag_remainder_key)) {
        return CLEARRA_STANDARD_BAG_AUTOMATON_INVALID_STATE;
    }
    *out_state = *state;
    if (out_state->bag_remainder_key != 0u) {
        return CLEARRA_STANDARD_BAG_AUTOMATON_OK;
    }
    if (out_state->cursor != 0u) {
        if (out_state->bag_epoch == UINT16_MAX) {
            return CLEARRA_STANDARD_BAG_AUTOMATON_INVALID_STATE;
        }
        out_state->bag_epoch = (uint16_t)(out_state->bag_epoch + 1u);
    }
    out_state->bag_remainder_key = clearra_standard_bag_full_remainder_key();
    return CLEARRA_STANDARD_BAG_AUTOMATON_OK;
}

ClearraStandardBagAutomatonStatus clearra_standard_bag_draw_piece(
    const clr_hold_automaton_state *state,
    uint8_t piece,
    ClearraStandardBagDraw *out_draw) {
    if (state == 0 || out_draw == 0 || piece < CLR_PIECE_I ||
        piece > CLR_PIECE_L) {
        return CLEARRA_STANDARD_BAG_AUTOMATON_INVALID_STATE;
    }
    clr_hold_automaton_state next;
    ClearraStandardBagAutomatonStatus status = refill_if_needed(state, &next);
    if (status != CLEARRA_STANDARD_BAG_AUTOMATON_OK) {
        return status;
    }
    uint64_t unit = piece_count_unit(piece);
    if ((next.bag_remainder_key & unit) == 0u) {
        return CLEARRA_STANDARD_BAG_AUTOMATON_PIECE_UNAVAILABLE;
    }
    if (next.cursor == UINT16_MAX) {
        return CLEARRA_STANDARD_BAG_AUTOMATON_INVALID_STATE;
    }
    next.bag_remainder_key -= unit;
    next.cursor = (uint16_t)(next.cursor + 1u);
    out_draw->piece = piece;
    out_draw->state = next;
    return CLEARRA_STANDARD_BAG_AUTOMATON_OK;
}

ClearraStandardBagAutomatonStatus clearra_standard_bag_enumerate_draws(
    const clr_hold_automaton_state *state,
    ClearraStandardBagDraw *out_draws,
    uint8_t capacity,
    uint8_t *out_count) {
    if (state == 0 || out_draws == 0 || out_count == 0) {
        return CLEARRA_STANDARD_BAG_AUTOMATON_INVALID_STATE;
    }
    *out_count = 0u;
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        ClearraStandardBagDraw draw;
        ClearraStandardBagAutomatonStatus status =
            clearra_standard_bag_draw_piece(state, piece, &draw);
        if (status == CLEARRA_STANDARD_BAG_AUTOMATON_PIECE_UNAVAILABLE) {
            continue;
        }
        if (status != CLEARRA_STANDARD_BAG_AUTOMATON_OK) {
            return status;
        }
        if (*out_count >= capacity) {
            return CLEARRA_STANDARD_BAG_AUTOMATON_CAPACITY_EXCEEDED;
        }
        out_draws[*out_count] = draw;
        *out_count = (uint8_t)(*out_count + 1u);
    }
    return CLEARRA_STANDARD_BAG_AUTOMATON_OK;
}
