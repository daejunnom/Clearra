#ifndef CLEARRA_BUILDUP_BFS_STATE_H
#define CLEARRA_BUILDUP_BFS_STATE_H

#include "clr_hold_automaton.h"
#include "clr_problem.h"

#include <stdbool.h>
#include <stdint.h>
typedef struct ClearraBuildUpState ClearraBuildUpState;typedef struct clr_deleted_line_state {
    uint16_t deleted_row_mask;
    uint8_t deleted_count;
    uint8_t reserved;
} clr_deleted_line_state;typedef struct clr_buildup_bfs_state {
    uint16_t remaining_ops_bitset;
    uint64_t current_board_mask;
    clr_deleted_line_state deleted_line_state;
    clr_hold_automaton_state hold_automaton_state;
    uint16_t piece_source_cursor;
    uint64_t reachability_relevant_state;
    uint8_t cleared_lines;
    uint8_t reserved[7];
} clr_buildup_bfs_state;bool clearra_buildup_bfs_state_has_deleted_line_state(
    const clr_buildup_bfs_state *state);
bool clearra_buildup_bfs_state_has_hold_automaton_state(
    const clr_buildup_bfs_state *state);
clr_buildup_status clearra_buildup_remaining_ops_bitset_for_count(
    uint16_t operation_count,
    uint16_t *out_bitset);
clr_buildup_bfs_state clearra_buildup_bfs_state_from_state(
    const ClearraBuildUpState *state,
    uint16_t remaining_ops_bitset);
#endif
