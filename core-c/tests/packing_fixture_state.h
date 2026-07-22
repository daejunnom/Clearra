#ifndef CLEARRA_PACKING_FIXTURE_STATE_H
#define CLEARRA_PACKING_FIXTURE_STATE_H

#include <stdint.h>

typedef struct ClearraPackingFixtureState {
    uint64_t board_mask;
    uint16_t cursor;
    uint8_t hold_piece;
    uint8_t hold_empty;
    uint16_t placed_pieces;
    uint8_t cleared_lines;
    uint8_t reserved;
} ClearraPackingFixtureState;

#endif
