#ifndef CLEARRA_BUILDUP_TESTS_SUPPORT_H
#define CLEARRA_BUILDUP_TESTS_SUPPORT_H

#include "../src/buildup/buildup_event.h"
#include "../src/buildup/buildup_bfs_state.h"
#include "../src/buildup/buildup_internal.h"
#include "../src/buildup/buildup_memo.h"
#include "../src/buildup/buildup_search_internal.h"
#include "../src/buildup/buildup_state.h"
#include "../src/buildup/generic_buildup.h"
#include "../src/buildup/buildup_worker.h"
#include "../src/cache/cache_identity.h"
#include "../src/packing/packing_problem.h"

#include <stdio.h>
#include <stdlib.h>

#define EXPECT_TRUE(EXPR)                                                                  \
    do {                                                                                   \
        if (!(EXPR)) {                                                                     \
            fprintf(stderr, "%s:%d expected true\n", __FILE__, __LINE__);                 \
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

#define EXPECT_BUILDUP_STATUS(EXPR, EXPECTED)                                              \
    do {                                                                                   \
        clr_buildup_status actual_status = (EXPR);                                         \
        if (actual_status != (EXPECTED)) {                                                 \
            fprintf(stderr, "%s:%d expected buildup status %d but got %d\n", __FILE__,     \
                    __LINE__, (int)(EXPECTED), (int)actual_status);                        \
            exit(1);                                                                       \
        }                                                                                  \
    } while (0)
void buildup_test_set_board_descriptor(
    clr_board_descriptor *descriptor,
    uint16_t width,
    uint16_t visible_height,
    uint16_t search_height,
    uint64_t initial_mask);
void buildup_test_set_piece_source_pattern_cache(
    clr_packing_problem *packing,
    const uint8_t *pieces,
    uint16_t count,
    uint8_t complete,
    uint16_t truncation_reason);
ClearraCacheIdentity buildup_test_full_cache_identity(void);
uint64_t buildup_test_low_mask_for_cells(uint32_t cell_count);
clr_packing_problem buildup_test_valid_packing_problem(void);
ClearraPackingCandidateView buildup_test_two_operation_candidate(void);
ClearraBoard64Layout buildup_test_standard_10x2_layout(void);
ClearraBoard64Layout buildup_test_standard_10x4_layout(void);
uint64_t buildup_test_cell_mask(ClearraBoard64Layout layout, uint8_t x, uint8_t y);
uint64_t buildup_test_o_mask_at(ClearraBoard64Layout layout, uint8_t x, uint8_t y);
uint64_t buildup_test_t_spawn_mask_at(ClearraBoard64Layout layout, uint8_t x, uint8_t y);
clr_packing_problem buildup_test_buildup_packing_problem(
    uint16_t height,
    uint16_t exact_pieces,
    uint8_t hold_enabled);
void buildup_test_set_packing_pieces(
    clr_packing_problem *packing,
    const uint8_t *pieces,
    uint16_t count,
    uint32_t source_kind,
    uint32_t provenance_id);
void buildup_test_configure_initial_hold(
    clr_buildup_problem *problem,
    uint8_t hold_enabled,
    uint8_t hold_empty,
    uint8_t hold_piece);
clr_rule_profile_descriptor buildup_test_rule_descriptor(uint32_t rule, uint32_t kick);
clr_rule_profile_descriptor buildup_test_imported_verified_kick_descriptor(void);
ClearraPackingCandidateView buildup_test_o_candidate_for_columns(
    ClearraBoard64Layout layout,
    const uint8_t *columns,
    uint8_t count);
ClearraPackingCandidateView buildup_test_representative_order_hint_is_not_solution_order_candidate(
    ClearraBoard64Layout layout);
clr_buildup_problem buildup_test_build_problem_from_candidate(
    clr_packing_problem packing,
    ClearraPackingCandidateView candidate);
void buildup_test_assert_buildup_reachability_bridge_uses_rule_kick_table(
    clr_rule_profile_descriptor rule,
    uint8_t piece,
    uint8_t rotation,
    uint32_t expected_kick_profile,
    bool expected_180_support);
clr_buildup_problem buildup_test_two_operation_gap_fill_problem(void);
#endif
