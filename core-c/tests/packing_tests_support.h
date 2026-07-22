#ifndef CLEARRA_PACKING_TESTS_SUPPORT_H
#define CLEARRA_PACKING_TESTS_SUPPORT_H

#include "../src/cache/cache_identity.h"
#include "../src/board/board64.h"
#include "../src/packing/packing_problem.h"
#include "packing_fixture_state.h"
#include "clr_piece.h"

#include <stdio.h>
#include <stdlib.h>

#define EXPECT_TRUE(EXPR)                                                                  \
    do {                                                                                   \
        if (!(EXPR)) {                                                                     \
            fprintf(stderr, "%s:%d expected true\n", __FILE__, __LINE__);                 \
            exit(1);                                                                       \
        }                                                                                  \
    } while (0)

#define EXPECT_FALSE(EXPR)                                                                 \
    do {                                                                                   \
        if ((EXPR)) {                                                                      \
            fprintf(stderr, "%s:%d expected false\n", __FILE__, __LINE__);                \
            exit(1);                                                                       \
        }                                                                                  \
    } while (0)

#define EXPECT_U64(EXPR, EXPECTED)                                                         \
    do {                                                                                   \
        uint64_t actual_value = (uint64_t)(EXPR);                                          \
        uint64_t expected_value = (uint64_t)(EXPECTED);                                    \
        if (actual_value != expected_value) {                                              \
            fprintf(stderr, "%s:%d expected 0x%llx but got 0x%llx\n", __FILE__, __LINE__,  \
                    (unsigned long long)expected_value,                                    \
                    (unsigned long long)actual_value);                                     \
            exit(1);                                                                       \
        }                                                                                  \
    } while (0)

#define EXPECT_STATUS(EXPR, EXPECTED)                                                      \
    do {                                                                                   \
        ClearraPackingStatus actual_status = (EXPR);                                      \
        ClearraPackingStatus expected_status = (EXPECTED);                                \
        if (actual_status != expected_status) {                                           \
            fprintf(stderr, "%s:%d expected status %d but got %d\n", __FILE__, __LINE__,   \
                    (int)expected_status, (int)actual_status);                            \
            exit(1);                                                                       \
        }                                                                                  \
    } while (0)
ClearraCacheIdentity packing_test_full_cache_identity(void);
ClearraBoard64Layout packing_test_standard_two_line_layout(void);
ClearraBoard64Layout packing_test_two_by_two_layout(void);
ClearraPackingCandidateView packing_test_single_operation_candidate(
    ClearraBoard64Layout layout,
    uint64_t mask,
    int8_t x);
clr_packing_problem packing_test_short_queue_problem(
    ClearraBoard64Layout layout,
    uint64_t target_mask,
    bool exact);
void packing_test_push_raw_candidate(
    ClearraPackingCandidateBuffer *buffer,
    ClearraPackingCandidateView candidate);
#endif
