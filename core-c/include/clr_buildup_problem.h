#ifndef CLR_BUILDUP_PROBLEM_H
#define CLR_BUILDUP_PROBLEM_H

#include "clr_buildup_operation.h"
#include "clr_hold_automaton.h"
#include "clr_packing_problem.h"
#define CLR_BUILDUP_SOURCE_CONCRETE_PATTERN 1u
#define CLR_BUILDUP_SOURCE_STANDARD_BAG_AUTOMATON 2u
#define CLR_BUILDUP_TERMINAL_PROJECTION_POLICY_VERSION 1u
#define CLR_BUILDUP_TERMINAL_PROJECTION_DISABLED 0u
#define CLR_BUILDUP_TERMINAL_PROJECTION_RELEASE_FINITE_HELD 1u
typedef struct ClearraGeometryCatalog ClearraGeometryCatalog;
typedef struct clr_buildup_problem {
    clr_packing_problem packing;
    clr_board_descriptor initial_board;
    clr_buildup_operation_set operation_set;
    const ClearraGeometryCatalog *geometry_catalog;
    uint64_t candidate_id;
    uint64_t canonical_operation_set_id;
    clr_piece_source_descriptor piece_source;
    uint8_t
        piece_source_pattern_pieces[CLR_PIECE_SOURCE_PATTERN_READER_CAPACITY];
    uint16_t piece_source_pattern_len;
    uint8_t piece_source_pattern_complete;
    uint8_t piece_source_pattern_reserved;
    uint16_t piece_source_pattern_truncation_reason;
    uint32_t piece_source_pattern_id;
    clr_hold_automaton_state initial_hold_automaton;
    clr_rule_profile_descriptor rule;
    uint32_t line_clear_policy;
    clr_piece_window_descriptor piece_window;
    uint32_t goal;
    uint32_t coverage_pattern_id;
    uint32_t buildup_flags;
    uint32_t source_execution_mode;
    uint16_t terminal_projection_policy_version;
    uint8_t terminal_projection_policy;
    uint8_t terminal_projection_reserved;
} clr_buildup_problem;clr_buildup_problem clr_buildup_problem_from_packing(clr_packing_problem problem);
bool clr_buildup_problem_is_valid(const clr_buildup_problem *problem);
clr_buildup_status clr_piece_source_pattern_piece_at(
    const clr_piece_source_pattern_reader *reader,
    const clr_hold_automaton_state *state,
    uint16_t cursor,
    uint8_t *out_piece);
clr_buildup_status clearra_buildup_runtime_status_for_board(
    const clr_board_descriptor *board);
#endif
