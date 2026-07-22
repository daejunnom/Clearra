#include "clr_hold_automaton.h"
static int clearra_hold_piece_present(uint8_t piece) {
    return piece >= CLR_PIECE_I && piece <= CLR_PIECE_L;
}uint32_t clearra_hold_automaton_apply(
    const clr_hold_automaton_state *state,
    uint32_t transition,
    uint8_t current_piece,
    uint8_t next_piece,
    clr_hold_automaton_step *out_step) {
    if (state == 0 || out_step == 0 || !clearra_hold_piece_present(current_piece)) {
        return CLR_HOLD_AUTOMATON_INVALID_ARGUMENT;
    }

    out_step->next_state = *state;
    out_step->used_piece = CLR_PIECE_NONE;

    switch (transition) {
    case CLR_HOLD_TRANSITION_USE_CURRENT:
        out_step->used_piece = current_piece;
        out_step->next_state.cursor = (uint16_t)(state->cursor + 1u);
        return CLR_HOLD_AUTOMATON_OK;

    case CLR_HOLD_TRANSITION_SWAP_HELD:
        if (!clearra_hold_piece_present(state->hold_piece) || state->hold_empty != 0u) {
            return CLR_HOLD_AUTOMATON_MISSING_HELD_PIECE;
        }
        out_step->used_piece = state->hold_piece;
        out_step->next_state.cursor = (uint16_t)(state->cursor + 1u);
        out_step->next_state.hold_piece = current_piece;
        out_step->next_state.hold_empty = 0u;
        return CLR_HOLD_AUTOMATON_OK;

    case CLR_HOLD_TRANSITION_STORE_CURRENT_THEN_USE_NEXT:
        if (!clearra_hold_piece_present(next_piece)) {
            return CLR_HOLD_AUTOMATON_MISSING_NEXT_PIECE;
        }
        out_step->used_piece = next_piece;
        out_step->next_state.cursor = (uint16_t)(state->cursor + 2u);
        out_step->next_state.hold_piece = current_piece;
        out_step->next_state.hold_empty = 0u;
        return CLR_HOLD_AUTOMATON_OK;

    default:
        return CLR_HOLD_AUTOMATON_UNKNOWN_TRANSITION;
    }
}