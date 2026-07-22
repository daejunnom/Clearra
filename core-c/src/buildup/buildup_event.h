#ifndef CLEARRA_BUILDUP_EVENT_H
#define CLEARRA_BUILDUP_EVENT_H

#include <stdint.h>
typedef enum ClearraBuildUpEventKind {
    CLEARRA_BUILDUP_EVENT_PLACEMENT = 1,
    CLEARRA_BUILDUP_EVENT_HOLD_SWAP = 2,
    CLEARRA_BUILDUP_EVENT_HOLD_STORE = 3,
    CLEARRA_BUILDUP_EVENT_LINE_CLEAR = 4
} ClearraBuildUpEventKind;typedef struct ClearraBuildUpEvent {
    ClearraBuildUpEventKind kind;
    uint8_t piece;
    uint8_t rotation;
    int16_t x;
    int16_t y;
    uint64_t board_before;
    uint64_t board_after;
    uint8_t cleared_lines;
    uint8_t reserved[7];
} ClearraBuildUpEvent;
#endif
