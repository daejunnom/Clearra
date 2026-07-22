#ifndef CLEARRA_BUILDUP_STATE_H
#define CLEARRA_BUILDUP_STATE_H

#include "clr_problem.h"
#include "clr_hold_automaton.h"

#include <stdint.h>
typedef struct ClearraLineClearState {
    uint16_t deleted_row_mask;
    uint8_t deleted_count;
    uint8_t reserved;
} ClearraLineClearState;

typedef struct ClearraBuildUpState {
    uint64_t board_mask;
    clr_hold_automaton_state hold_automaton_state;
    uint64_t reachability_relevant_state;
    ClearraLineClearState line_clear_state;
    uint16_t placed_pieces;
    uint8_t cleared_lines;
    uint8_t last_hold_branch_kind;
} ClearraBuildUpState;

ClearraBuildUpState clearra_buildup_state_initial(
    const clr_buildup_problem *problem);

_Static_assert(
    sizeof(ClearraBuildUpState) == 64u,
    "BuildUp hot state must fit one 64-byte cache line");
#endif
