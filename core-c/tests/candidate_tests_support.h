#ifndef CLEARRA_CANDIDATE_TESTS_SUPPORT_H
#define CLEARRA_CANDIDATE_TESTS_SUPPORT_H

#include "../src/reachability/reachability.h"

#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>

#define EXPECT_STATUS(EXPR, EXPECTED)                                                       \
    do {                                                                                    \
        ClearraCandidateStatus actual_status = (EXPR);                                      \
        if (actual_status != (EXPECTED)) {                                                  \
            fprintf(stderr, "%s:%d expected candidate status %d but got %d\n", __FILE__,    \
                    __LINE__, (int)(EXPECTED), (int)actual_status);                         \
            exit(1);                                                                        \
        }                                                                                   \
    } while (0)

#define EXPECT_BOARD_STATUS(EXPR, EXPECTED)                                                \
    do {                                                                                    \
        ClearraBoard64Status actual_status = (EXPR);                                        \
        if (actual_status != (EXPECTED)) {                                                  \
            fprintf(stderr, "%s:%d expected board status %d but got %d\n", __FILE__,        \
                    __LINE__, (int)(EXPECTED), (int)actual_status);                         \
            exit(1);                                                                        \
        }                                                                                   \
    } while (0)

#define EXPECT_U64(EXPR, EXPECTED)                                                         \
    do {                                                                                    \
        uint64_t actual_value = (uint64_t)(EXPR);                                           \
        uint64_t expected_value = (uint64_t)(EXPECTED);                                     \
        if (actual_value != expected_value) {                                               \
            fprintf(stderr, "%s:%d expected 0x%llx but got 0x%llx\n", __FILE__, __LINE__,   \
                    (unsigned long long)expected_value,                                     \
                    (unsigned long long)actual_value);                                      \
            exit(1);                                                                        \
        }                                                                                   \
    } while (0)

#define EXPECT_I8(EXPR, EXPECTED)                                                          \
    do {                                                                                    \
        int actual_value = (int)(EXPR);                                                     \
        int expected_value = (int)(EXPECTED);                                               \
        if (actual_value != expected_value) {                                               \
            fprintf(stderr, "%s:%d expected %d but got %d\n", __FILE__, __LINE__,           \
                    expected_value, actual_value);                                          \
            exit(1);                                                                        \
        }                                                                                   \
    } while (0)

#define EXPECT_TRUE(EXPR)                                                                  \
    do {                                                                                    \
        if (!(EXPR)) {                                                                      \
            fprintf(stderr, "%s:%d expected true\n", __FILE__, __LINE__);                  \
            exit(1);                                                                        \
        }                                                                                   \
    } while (0)

#define EXPECT_FALSE(EXPR)                                                                 \
    do {                                                                                    \
        if ((EXPR)) {                                                                       \
            fprintf(stderr, "%s:%d expected false\n", __FILE__, __LINE__);                 \
            exit(1);                                                                        \
        }                                                                                   \
    } while (0)
ClearraBoard64Layout candidate_test_standard_10x4(void);
ClearraCacheIdentity candidate_test_full_cache_identity(void);
clr_rule_profile_descriptor candidate_test_rule_descriptor(uint32_t rule, uint32_t kick);
ClearraCompactRuleProfile candidate_test_compact_rule(uint32_t rule, uint32_t kick);
uint64_t candidate_test_cell_mask(ClearraBoard64Layout layout, uint8_t x, uint8_t y);
uint64_t candidate_test_half_turn_only_board(ClearraBoard64Layout layout);
void candidate_test_candidate_fixture_kick_table(
    ClearraReachabilityKickTable *out_table);
bool candidate_test_candidate_list_contains(
    const ClearraCandidateList *list,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y);
bool candidate_test_candidate_list_has_transition(
    const ClearraCandidateList *list,
    uint8_t transition_kind);
bool candidate_test_candidate_list_contains_with_transition(
    const ClearraCandidateList *list,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    uint8_t transition_kind);
#endif
