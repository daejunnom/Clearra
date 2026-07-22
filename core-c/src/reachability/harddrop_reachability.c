#include "reachability_field.h"

ClearraReachabilityStatus clearra_harddrop_reachability_is_reachable(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    bool *out_reachable) {
    return clearra_reachability_field_has_harddrop_path(
        layout, board, piece, rotation, x, y, out_reachable);
}
