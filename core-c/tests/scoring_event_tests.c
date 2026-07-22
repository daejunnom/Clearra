#include "../src/scoring_events/scoring_event_basis.h"

#include <stdio.h>
#include <stdlib.h>

#define EXPECT_STATUS(EXPR, EXPECTED)                                                    \
    do {                                                                                 \
        ClearraScoringEventStatus actual_status = (EXPR);                                \
        if (actual_status != (EXPECTED)) {                                               \
            fprintf(stderr, "%s:%d expected status %d but got %d\n", __FILE__, __LINE__, \
                    (int)(EXPECTED), (int)actual_status);                                \
            exit(1);                                                                     \
        }                                                                                \
    } while (0)

#define EXPECT_U64(EXPR, EXPECTED)                                                       \
    do {                                                                                 \
        unsigned long long actual_value = (unsigned long long)(EXPR);                    \
        unsigned long long expected_value = (unsigned long long)(EXPECTED);              \
        if (actual_value != expected_value) {                                            \
            fprintf(stderr, "%s:%d expected %llu but got %llu\n", __FILE__, __LINE__,    \
                    expected_value, actual_value);                                       \
            exit(1);                                                                     \
        }                                                                                \
    } while (0)
static void placement_event_available(void) {
    ClearraScoringPlacementEvent event;
    EXPECT_STATUS(clearra_scoring_placement_event_make(3, 1, 0, 4, 1, 0x30, &event),
                  CLEARRA_SCORING_EVENT_OK);
    EXPECT_U64(event.step_index, 3);
    EXPECT_U64(event.piece, 1);
    EXPECT_U64(event.placed_mask, 0x30);
}static void clear_event_available(void) {
    ClearraScoringClearEvent event;
    EXPECT_STATUS(clearra_scoring_clear_event_make(2, 4, 1, &event),
                  CLEARRA_SCORING_EVENT_OK);
    EXPECT_U64(event.cleared_lines, 4);
    EXPECT_U64(event.perfect_clear, 1);
}static void drop_event_basis_available(void) {
    ClearraScoringDropEvent event;
    EXPECT_STATUS(clearra_scoring_drop_event_make(1, 20, 3, &event),
                  CLEARRA_SCORING_EVENT_OK);
    EXPECT_U64(event.from_y, 20);
    EXPECT_U64(event.to_y, 3);
    EXPECT_U64(event.distance, 17);
}static void spin_event_basis_available(void) {
    ClearraScoringSpinBasisEvent event;
    EXPECT_STATUS(clearra_scoring_spin_basis_event_make(
                      4, 3, 1, 5, 7, 0x0100, 0x0300, 2, &event),
                  CLEARRA_SCORING_EVENT_OK);
    EXPECT_U64(event.step_index, 4);
    EXPECT_U64(event.board_before, 0x0100);
    EXPECT_U64(event.board_after_placement, 0x0300);
    EXPECT_U64(event.cleared_lines, 2);
}int main(void) {
    placement_event_available();
    clear_event_available();
    drop_event_basis_available();
    spin_event_basis_available();
    puts("core-c scoring event tests passed");
    return 0;
}