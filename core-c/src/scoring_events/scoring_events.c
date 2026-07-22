#include "scoring_event_basis.h"

ClearraScoringEventStatus clearra_scoring_placement_event_make(
    uint16_t step_index,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    uint64_t placed_mask,
    ClearraScoringPlacementEvent *out_event) {
    if (out_event == 0 || piece == 0) {
        return CLEARRA_SCORING_EVENT_INVALID_ARGUMENT;
    }
    *out_event = (ClearraScoringPlacementEvent){
        step_index, piece, rotation, x, y, placed_mask,
    };
    return CLEARRA_SCORING_EVENT_OK;
}

ClearraScoringEventStatus clearra_scoring_clear_event_make(
    uint16_t step_index,
    uint8_t cleared_lines,
    uint8_t perfect_clear,
    ClearraScoringClearEvent *out_event) {
    if (out_event == 0 || cleared_lines > 4) {
        return CLEARRA_SCORING_EVENT_INVALID_ARGUMENT;
    }
    *out_event = (ClearraScoringClearEvent){
        step_index, cleared_lines, perfect_clear ? 1u : 0u,
    };
    return CLEARRA_SCORING_EVENT_OK;
}

ClearraScoringEventStatus clearra_scoring_drop_event_make(
    uint16_t step_index,
    int16_t from_y,
    int16_t to_y,
    ClearraScoringDropEvent *out_event) {
    if (out_event == 0) {
        return CLEARRA_SCORING_EVENT_INVALID_ARGUMENT;
    }
    int16_t distance = from_y > to_y ? (int16_t)(from_y - to_y) : 0;
    *out_event = (ClearraScoringDropEvent){
        step_index, from_y, to_y, (uint16_t)distance,
    };
    return CLEARRA_SCORING_EVENT_OK;
}

ClearraScoringEventStatus clearra_scoring_spin_basis_event_make(
    uint16_t step_index,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    uint64_t board_before,
    uint64_t board_after_placement,
    uint8_t cleared_lines,
    ClearraScoringSpinBasisEvent *out_event) {
    if (out_event == 0 || piece == 0 || cleared_lines > 4) {
        return CLEARRA_SCORING_EVENT_INVALID_ARGUMENT;
    }
    *out_event = (ClearraScoringSpinBasisEvent){
        step_index, piece, rotation, x, y, board_before,
        board_after_placement, cleared_lines,
    };
    return CLEARRA_SCORING_EVENT_OK;
}
