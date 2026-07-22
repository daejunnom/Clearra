#ifndef CLR_PACKING_PROBLEM_H
#define CLR_PACKING_PROBLEM_H

#include "clr_board.h"
#include "clr_piece.h"
#include "clr_piece_source.h"
#include "clr_problem_budget.h"
#include "clr_problem_policy.h"
#include "clr_rules.h"

#include <stdbool.h>
#include <stdint.h>
typedef struct clr_packing_problem {
    uint32_t problem_kind;
    uint16_t max_pieces;
    uint16_t flags;
    clr_board_descriptor board;
    uint64_t goal_region_mask;
    uint64_t required_fill_mask;
    uint64_t forbidden_mask;
    uint16_t exact_pieces;
    uint16_t reserved_goal;
    clr_piece_window_descriptor piece_window;
    clr_piece_multiset_window piece_multiset_window;
    clr_piece_multiset_family piece_multiset_family;
    clr_piece_source_descriptor piece_source;
    uint8_t
        piece_source_pattern_pieces[CLR_PIECE_SOURCE_PATTERN_READER_CAPACITY];
    uint16_t piece_source_pattern_len;
    uint8_t piece_source_pattern_complete;
    uint8_t piece_source_pattern_reserved;
    uint16_t piece_source_pattern_truncation_reason;
    uint32_t piece_source_pattern_id;
    clr_rule_profile_descriptor rule;
    clr_problem_budget budget;
    clr_backend_request backend;
    clr_checkpoint_spec checkpoint;
    uint32_t goal;
    uint32_t count_policy;
    uint32_t objective;
    uint32_t label_count;
} clr_packing_problem;typedef clr_packing_problem ClearraPackingProblem;clr_packing_problem clr_packing_problem_zero(void);
bool clr_packing_problem_is_valid(const clr_packing_problem *problem);
#endif
