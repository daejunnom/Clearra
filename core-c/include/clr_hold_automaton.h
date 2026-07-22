#ifndef CLR_HOLD_AUTOMATON_H
#define CLR_HOLD_AUTOMATON_H

#include "clr_piece.h"

#include <stdint.h>

#define CLR_HOLD_TRANSITION_USE_CURRENT 1u
#define CLR_HOLD_TRANSITION_SWAP_HELD 2u
#define CLR_HOLD_TRANSITION_STORE_CURRENT_THEN_USE_NEXT 3u

#define CLR_HOLD_AUTOMATON_OK 0u
#define CLR_HOLD_AUTOMATON_INVALID_ARGUMENT 1u
#define CLR_HOLD_AUTOMATON_MISSING_HELD_PIECE 2u
#define CLR_HOLD_AUTOMATON_MISSING_NEXT_PIECE 3u
#define CLR_HOLD_AUTOMATON_UNKNOWN_TRANSITION 4u
typedef struct clr_hold_automaton_state {
    uint64_t piece_source_id;
    uint16_t cursor;
    uint16_t bag_epoch;
    uint64_t bag_remainder_key;
    uint64_t provenance_id;
    uint8_t hold_piece;
    uint8_t hold_empty;
    uint8_t reserved[6];
} clr_hold_automaton_state;typedef struct clr_hold_automaton_step {
    uint8_t used_piece;
    clr_hold_automaton_state next_state;
} clr_hold_automaton_step;typedef struct clr_buildup_hold_automaton_memo_key {
    uint64_t piece_source_id;
    uint16_t cursor;
    uint16_t bag_epoch;
    uint64_t bag_remainder_key;
    uint64_t provenance_id;
    uint8_t hold_piece;
    uint8_t hold_empty;
    uint8_t reserved[6];
} clr_buildup_hold_automaton_memo_key;uint32_t clearra_hold_automaton_apply(
    const clr_hold_automaton_state *state,
    uint32_t transition,
    uint8_t current_piece,
    uint8_t next_piece,
    clr_hold_automaton_step *out_step);clr_buildup_hold_automaton_memo_key clearra_buildup_hold_automaton_memo_key(
    const clr_hold_automaton_state *state);uint64_t clearra_buildup_hold_automaton_memo_key_hash(
    const clr_buildup_hold_automaton_memo_key *key);
#endif
