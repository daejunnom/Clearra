#include "operation.h"
bool clearra_rotation_state_is_valid(uint8_t rotation) {
    return rotation < CLEARRA_ROTATION_STATE_COUNT;
}const char *clearra_rotation_state_name(uint8_t rotation) {
    switch (rotation) {
        case CLEARRA_ROTATION_SPAWN:
            return "spawn";
        case CLEARRA_ROTATION_RIGHT:
            return "right";
        case CLEARRA_ROTATION_REVERSE:
            return "reverse";
        case CLEARRA_ROTATION_LEFT:
            return "left";
        default:
            return "invalid";
    }
}uint8_t clearra_rotation_count_for_piece(uint8_t piece) {
    return clearra_piece_is_standard_tetromino(piece) ? CLEARRA_ROTATION_STATE_COUNT : 0u;
}