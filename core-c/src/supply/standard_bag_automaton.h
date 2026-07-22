#ifndef CLEARRA_STANDARD_BAG_AUTOMATON_H
#define CLEARRA_STANDARD_BAG_AUTOMATON_H

#include "clr_hold_automaton.h"

#include <stdbool.h>
#include <stdint.h>

#define CLEARRA_STANDARD_BAG_DRAW_CAPACITY 7u

typedef enum ClearraStandardBagAutomatonStatus {
    CLEARRA_STANDARD_BAG_AUTOMATON_OK = 0,
    CLEARRA_STANDARD_BAG_AUTOMATON_INVALID_STATE = 1,
    CLEARRA_STANDARD_BAG_AUTOMATON_PIECE_UNAVAILABLE = 2,
    CLEARRA_STANDARD_BAG_AUTOMATON_CAPACITY_EXCEEDED = 3
} ClearraStandardBagAutomatonStatus;

typedef struct ClearraStandardBagDraw {
    uint8_t piece;
    clr_hold_automaton_state state;
} ClearraStandardBagDraw;

uint64_t clearra_standard_bag_full_remainder_key(void);
bool clearra_standard_bag_remainder_key_is_exact(uint64_t key);
ClearraStandardBagAutomatonStatus clearra_standard_bag_draw_piece(
    const clr_hold_automaton_state *state,
    uint8_t piece,
    ClearraStandardBagDraw *out_draw);
ClearraStandardBagAutomatonStatus clearra_standard_bag_enumerate_draws(
    const clr_hold_automaton_state *state,
    ClearraStandardBagDraw *out_draws,
    uint8_t capacity,
    uint8_t *out_count);

#endif
