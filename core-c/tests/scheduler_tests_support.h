#ifndef CLEARRA_SCHEDULER_TESTS_SUPPORT_H
#define CLEARRA_SCHEDULER_TESTS_SUPPORT_H

#include "hybrid_support/hybrid_scheduler.h"

#include <stdio.h>
#include <stdlib.h>

#define EXPECT_HYBRID_STATUS(EXPR, EXPECTED)                                             \
    do {                                                                                 \
        ClearraHybridStatus actual_status = (EXPR);                                      \
        if (actual_status != (EXPECTED)) {                                               \
            fprintf(stderr, "%s:%d expected hybrid status %d but got %d\n", __FILE__,    \
                    __LINE__, (int)(EXPECTED), (int)actual_status);                      \
            exit(1);                                                                     \
        }                                                                                \
    } while (0)

#define EXPECT_GPU_STATUS(EXPR, EXPECTED)                                                \
    do {                                                                                 \
        ClearraGpuStatus actual_status = (EXPR);                                         \
        if (actual_status != (EXPECTED)) {                                               \
            fprintf(stderr, "%s:%d expected gpu status %d but got %d\n", __FILE__,       \
                    __LINE__, (int)(EXPECTED), (int)actual_status);                      \
            exit(1);                                                                     \
        }                                                                                \
    } while (0)

#define EXPECT_MEM_STATUS(EXPR, EXPECTED)                                                \
    do {                                                                                 \
        ClrMemStatus actual_status = (EXPR);                                             \
        if (actual_status != (EXPECTED)) {                                               \
            fprintf(stderr, "%s:%d expected memory status %d but got %d\n", __FILE__,    \
                    __LINE__, (int)(EXPECTED), (int)actual_status);                      \
            exit(1);                                                                     \
        }                                                                                \
    } while (0)

#define EXPECT_PACKING_STATUS(EXPR, EXPECTED)                                            \
    do {                                                                                 \
        ClearraPackingStatus actual_status = (EXPR);                                     \
        if (actual_status != (EXPECTED)) {                                               \
            fprintf(stderr, "%s:%d expected packing status %d but got %d\n", __FILE__,   \
                    __LINE__, (int)(EXPECTED), (int)actual_status);                      \
            exit(1);                                                                     \
        }                                                                                \
    } while (0)

#define EXPECT_TRUE(EXPR)                                                                \
    do {                                                                                 \
        if (!(EXPR)) {                                                                   \
            fprintf(stderr, "%s:%d expected true\n", __FILE__, __LINE__);               \
            exit(1);                                                                     \
        }                                                                                \
    } while (0)

#define EXPECT_FALSE(EXPR)                                                               \
    do {                                                                                 \
        if ((EXPR)) {                                                                    \
            fprintf(stderr, "%s:%d expected false\n", __FILE__, __LINE__);              \
            exit(1);                                                                     \
        }                                                                                \
    } while (0)

#define EXPECT_U64(EXPR, EXPECTED)                                                       \
    do {                                                                                 \
        uint64_t actual_value = (uint64_t)(EXPR);                                        \
        uint64_t expected_value = (uint64_t)(EXPECTED);                                  \
        if (actual_value != expected_value) {                                            \
            fprintf(stderr, "%s:%d expected %llu but got %llu\n", __FILE__, __LINE__,    \
                    (unsigned long long)expected_value,                                  \
                    (unsigned long long)actual_value);                                   \
            exit(1);                                                                     \
        }                                                                                \
    } while (0)
void scheduler_test_set_board_descriptor(
    clr_board_descriptor *descriptor,
    uint16_t width,
    uint16_t visible_height,
    uint16_t search_height,
    uint64_t initial_mask);
ClearraBoard64Layout scheduler_test_two_line_layout(void);
void scheduler_test_scheduler_batch_into(ClearraGpuPackingBatchDescriptor *out_batch);
ClearraGpuPackingBatchDescriptor scheduler_test_scheduler_batch(void);
uint64_t scheduler_test_low_mask_for_cells(uint32_t cell_count);
void scheduler_test_scheduler_packing_problem_into(clr_packing_problem *out_problem);
clr_packing_problem scheduler_test_scheduler_packing_problem(void);
#endif
