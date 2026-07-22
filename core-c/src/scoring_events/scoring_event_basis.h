#ifndef CLEARRA_SCORING_EVENT_BASIS_H
#define CLEARRA_SCORING_EVENT_BASIS_H

#include <stdint.h>
typedef enum ClearraScoringEventStatus {
    CLEARRA_SCORING_EVENT_OK = 0,
    CLEARRA_SCORING_EVENT_INVALID_ARGUMENT = 1
} ClearraScoringEventStatus;typedef struct ClearraScoringPlacementEvent {
    uint16_t step_index;
    uint8_t piece;
    uint8_t rotation;
    int8_t x;
    int8_t y;
    uint64_t placed_mask;
} ClearraScoringPlacementEvent;typedef struct ClearraScoringClearEvent {
    uint16_t step_index;
    uint8_t cleared_lines;
    uint8_t perfect_clear;
} ClearraScoringClearEvent;typedef struct ClearraScoringDropEvent {
    uint16_t step_index;
    int16_t from_y;
    int16_t to_y;
    uint16_t distance;
} ClearraScoringDropEvent;typedef struct ClearraScoringSpinBasisEvent {
    uint16_t step_index;
    uint8_t piece;
    uint8_t rotation;
    int8_t x;
    int8_t y;
    uint64_t board_before;
    uint64_t board_after_placement;
    uint8_t cleared_lines;
} ClearraScoringSpinBasisEvent;ClearraScoringEventStatus clearra_scoring_placement_event_make(
    uint16_t step_index,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    uint64_t placed_mask,
    ClearraScoringPlacementEvent *out_event);
ClearraScoringEventStatus clearra_scoring_clear_event_make(
    uint16_t step_index,
    uint8_t cleared_lines,
    uint8_t perfect_clear,
    ClearraScoringClearEvent *out_event);
ClearraScoringEventStatus clearra_scoring_drop_event_make(
    uint16_t step_index,
    int16_t from_y,
    int16_t to_y,
    ClearraScoringDropEvent *out_event);
ClearraScoringEventStatus clearra_scoring_spin_basis_event_make(
    uint16_t step_index,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    uint64_t board_before,
    uint64_t board_after_placement,
    uint8_t cleared_lines,
    ClearraScoringSpinBasisEvent *out_event);
#endif
