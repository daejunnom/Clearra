#include "clr_problem.h"
#include "clr_supply.h"

clr_hold_state clearra_hold_state_empty(uint8_t enabled) {
    clr_hold_state hold = {0};
    hold.enabled = enabled ? 1u : 0u;
    hold.empty = 1u;
    return hold;
}

clr_hold_state clearra_hold_state_occupied(uint8_t piece) {
    clr_hold_state hold = {0};
    hold.enabled = 1u;
    hold.empty = 0u;
    hold.piece = piece;
    return hold;
}

bool clearra_hold_state_has_piece(const clr_hold_state *hold) {
    return hold != 0 && hold->enabled != 0u && hold->empty == 0u &&
        hold->piece != 0u;
}

clr_bag_window clearra_bag_window_from_queue_and_piece_window(
    const clr_queue_view *queue,
    const clr_piece_window_descriptor *piece_window) {
    clr_bag_window window = {0};
    if (queue == 0 || piece_window == 0) {
        return window;
    }
    window.start = piece_window->has_exact_pieces ? 0u : piece_window->max_pieces;
    window.len = queue->len;
    window.boundary_known =
        (uint8_t)(queue->mode == CLR_QUEUE_BAG_ALIGNED_PATTERN);
    return window;
}

bool clearra_bag_window_boundary_known(const clr_bag_window *window) {
    return window != 0 && window->boundary_known != 0u;
}
