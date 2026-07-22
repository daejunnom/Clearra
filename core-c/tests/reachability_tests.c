#include "../src/reachability/reachability.h"
#include "../src/reachability/reachable_lock_batch.h"

#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>

#define EXPECT_REACH_STATUS(EXPR, EXPECTED)                                               \
    do {                                                                                  \
        ClearraReachabilityStatus actual_status = (EXPR);                                 \
        if (actual_status != (EXPECTED)) {                                                \
            fprintf(stderr, "%s:%d expected reachability status %d but got %d\n",         \
                    __FILE__, __LINE__, (int)(EXPECTED), (int)actual_status);             \
            exit(1);                                                                      \
        }                                                                                 \
    } while (0)

#define EXPECT_CANDIDATE_STATUS(EXPR, EXPECTED)                                           \
    do {                                                                                  \
        ClearraCandidateStatus actual_status = (EXPR);                                    \
        if (actual_status != (EXPECTED)) {                                                \
            fprintf(stderr, "%s:%d expected candidate status %d but got %d\n",            \
                    __FILE__, __LINE__, (int)(EXPECTED), (int)actual_status);             \
            exit(1);                                                                      \
        }                                                                                 \
    } while (0)

#define EXPECT_BOARD_STATUS(EXPR, EXPECTED)                                               \
    do {                                                                                  \
        ClearraBoard64Status actual_status = (EXPR);                                      \
        if (actual_status != (EXPECTED)) {                                                \
            fprintf(stderr, "%s:%d expected board status %d but got %d\n", __FILE__,      \
                    __LINE__, (int)(EXPECTED), (int)actual_status);                       \
            exit(1);                                                                      \
        }                                                                                 \
    } while (0)

#define EXPECT_TRUE(EXPR)                                                                \
    do {                                                                                  \
        if (!(EXPR)) {                                                                    \
            fprintf(stderr, "%s:%d expected true\n", __FILE__, __LINE__);                \
            exit(1);                                                                      \
        }                                                                                 \
    } while (0)

#define EXPECT_FALSE(EXPR)                                                               \
    do {                                                                                  \
        if ((EXPR)) {                                                                     \
            fprintf(stderr, "%s:%d expected false\n", __FILE__, __LINE__);               \
            exit(1);                                                                      \
        }                                                                                 \
    } while (0)

#define EXPECT_U64(EXPR, EXPECTED)                                                       \
    do {                                                                                  \
        uint64_t actual_value = (uint64_t)(EXPR);                                         \
        uint64_t expected_value = (uint64_t)(EXPECTED);                                   \
        if (actual_value != expected_value) {                                             \
            fprintf(stderr, "%s:%d expected 0x%llx but got 0x%llx\n", __FILE__,           \
                    __LINE__, (unsigned long long)expected_value,                         \
                    (unsigned long long)actual_value);                                    \
            exit(1);                                                                      \
        }                                                                                 \
    } while (0)
static ClearraBoard64Layout standard_10x4(void) {
    ClearraBoard64Layout layout;
    EXPECT_BOARD_STATUS(clearra_board64_make_layout(10, 4, &layout), CLEARRA_BOARD64_OK);
    return layout;
}static ClearraCacheIdentity full_cache_identity(void) {
    ClearraCacheIdentity identity = clearra_cache_identity_zero();
    identity.board = clearra_board64_empty();
    identity.piece_set_profile = 1;
    identity.piece_definition_id_fingerprint = 11;
    identity.piece_area_multiset_fingerprint = 12;
    identity.rule_kick_profile = 2;
    identity.backend_mode = 3;
    identity.operation_table_version = 4;
    identity.supply_provenance = 5;
    identity.queue_pattern_id = 6;
    identity.piece_window_start = 0;
    identity.piece_window_len = 5;
    identity.goal_id = 7;
    return identity;
}static uint64_t cell_mask(ClearraBoard64Layout layout, uint8_t x, uint8_t y) {
    uint8_t index = 0;
    EXPECT_BOARD_STATUS(clearra_board64_cell_index(layout, x, y, &index),
                        CLEARRA_BOARD64_OK);
    return UINT64_C(1) << index;
}static uint64_t half_turn_only_board(ClearraBoard64Layout layout) {
    (void)layout;
    return UINT64_C(0x8099802143);
}static void print_reachability_debug_path(const ClearraReachabilityReport *report) {
    fprintf(stderr, "reachability debug path (%u steps):", report->debug_step_count);
    for (uint8_t index = 0; index < report->debug_step_count; index++) {
        const ClearraReachabilityDebugStep *step = &report->debug_steps[index];
        fprintf(stderr, " [r=%u x=%d y=%d t=%u]", (unsigned)step->rotation,
                (int)step->x, (int)step->y, (unsigned)step->transition_kind);
    }
    fprintf(stderr, "\n");
}static ClearraReachabilityKickTable simple_kick_table(void) {
    static const ClearraKickOffset clockwise_offsets[2] = {{-1, 1}, {1, 1}};
    static const ClearraKickOffset counter_clockwise_offsets[1] = {{0, 0}};
    static const ClearraKickOffset half_turn_offsets[2] = {{0, 0}, {1, 0}};
    ClearraReachabilityKickTable table = {0};
    table.clockwise_offsets = clockwise_offsets;
    table.clockwise_count = 2;
    table.counter_clockwise_offsets = counter_clockwise_offsets;
    table.counter_clockwise_count = 1;
    table.half_turn_offsets = half_turn_offsets;
    table.half_turn_count = 2;
    return table;
}static ClearraReachabilityKickTable reverse_predecessor_kick_table(void) {
    static const ClearraKickOffset clockwise_offsets[1] = {{-2, 1}};
    ClearraReachabilityKickTable table = {0};
    table.clockwise_offsets = clockwise_offsets;
    table.clockwise_count = 1;
    return table;
}static ClearraReachabilityKickTable srs_plus_kick_table(uint8_t piece) {
    ClearraReachabilityKickTable table = {0};
    EXPECT_TRUE(clearra_srs_plus_kick_table(&table.owned_compact_table) ==
                CLEARRA_RULE_OK);
    table.compact_table = &table.owned_compact_table;
    table.piece = piece;
    return table;
}static void collision_free_but_unreachable_fixture(void) {
    ClearraBoard64Layout layout = standard_10x4();
    bool reachable = true;

    EXPECT_REACH_STATUS(clearra_harddrop_reachability_is_reachable(
                            layout, clearra_board64_empty(), CLEARRA_CANDIDATE_PIECE_O,
                            CLEARRA_CANDIDATE_ROTATION_ZERO, 4, 2, &reachable),
                        CLEARRA_REACHABILITY_OK);
    EXPECT_FALSE(reachable);
}static void harddrop_reachable_fixture(void) {
    ClearraBoard64Layout layout = standard_10x4();
    ClearraReachabilityReport report;

    EXPECT_REACH_STATUS(clearra_reachability_check(
                            layout, clearra_board64_empty(), CLEARRA_CANDIDATE_PIECE_O,
                            CLEARRA_CANDIDATE_ROTATION_ZERO, 4, 0,
                            CLEARRA_REACHABILITY_MODE_HARDDROP, 0, &report),
                        CLEARRA_REACHABILITY_OK);
    EXPECT_TRUE(report.reachable);
    EXPECT_U64(report.visited_states, 1);
}static void locked_reachability_rejects_ungrounded_collision_free_target(void) {
    ClearraBoard64Layout layout = standard_10x4();
    ClearraReachabilityReport report;

    EXPECT_REACH_STATUS(clearra_reachability_check(
                            layout, clearra_board64_empty(), CLEARRA_CANDIDATE_PIECE_O,
                            CLEARRA_CANDIDATE_ROTATION_ZERO, 4, 2,
                            CLEARRA_REACHABILITY_MODE_LOCKED, 0, &report),
                        CLEARRA_REACHABILITY_OK);
    EXPECT_FALSE(report.reachable);
}static void locked_reachable_via_multiple_movements_fixture(void) {
    ClearraBoard64Layout layout = standard_10x4();
    ClearraReachabilityReport report;
    uint64_t board = cell_mask(layout, 0, 2);

    EXPECT_REACH_STATUS(clearra_reachability_check(
                            layout, board, CLEARRA_CANDIDATE_PIECE_O,
                            CLEARRA_CANDIDATE_ROTATION_ZERO, 0, 0,
                            CLEARRA_REACHABILITY_MODE_HARDDROP, 0, &report),
                        CLEARRA_REACHABILITY_OK);
    EXPECT_FALSE(report.reachable);

    EXPECT_REACH_STATUS(clearra_reachability_check(
                            layout, board, CLEARRA_CANDIDATE_PIECE_O,
                            CLEARRA_CANDIDATE_ROTATION_ZERO, 0, 0,
                            CLEARRA_REACHABILITY_MODE_LOCKED, 0, &report),
                        CLEARRA_REACHABILITY_OK);
    EXPECT_TRUE(report.reachable);
    EXPECT_TRUE(report.visited_states > 1);
}static void kick_reachable_only_with_first_success_offset_fixture(void) {
    ClearraBoard64Layout layout = standard_10x4();
    ClearraReachabilityKickTable table = simple_kick_table();
    ClearraCandidateOperation operation;
    uint64_t colliding_mask = 0;

    EXPECT_CANDIDATE_STATUS(clearra_candidate_mask_for_piece(
                                layout, CLEARRA_CANDIDATE_PIECE_T,
                                CLEARRA_CANDIDATE_ROTATION_RIGHT, 0, 0,
                                &colliding_mask),
                            CLEARRA_CANDIDATE_OK);
    EXPECT_REACH_STATUS(clearra_kick_first_success(
                            layout, colliding_mask, CLEARRA_CANDIDATE_PIECE_T,
                            CLEARRA_CANDIDATE_ROTATION_ZERO,
                            CLEARRA_CANDIDATE_ROTATION_RIGHT, 0, 0, &table,
                            &operation),
                        CLEARRA_REACHABILITY_OK);

    EXPECT_U64(operation.kick_index, 1);
    EXPECT_U64(operation.x, 2);
    EXPECT_U64(operation.transition_kind, CLEARRA_ROTATION_TRANSITION_CLOCKWISE);
}static void kick_order_mismatch_rejected_fixture(void) {
    ClearraBoard64Layout layout = standard_10x4();
    ClearraReachabilityKickTable table = simple_kick_table();
    ClearraCandidateOperation operation;
    uint64_t colliding_mask = 0;

    EXPECT_CANDIDATE_STATUS(clearra_candidate_mask_for_piece(
                                layout, CLEARRA_CANDIDATE_PIECE_T,
                                CLEARRA_CANDIDATE_ROTATION_RIGHT, 0, 0,
                                &colliding_mask),
                            CLEARRA_CANDIDATE_OK);
    EXPECT_REACH_STATUS(clearra_kick_first_success(
                            layout, colliding_mask, CLEARRA_CANDIDATE_PIECE_T,
                            CLEARRA_CANDIDATE_ROTATION_ZERO,
                            CLEARRA_CANDIDATE_ROTATION_RIGHT, 0, 0, &table,
                            &operation),
                        CLEARRA_REACHABILITY_OK);

    EXPECT_FALSE(operation.kick_index == 0);
    EXPECT_U64(operation.kick_index, 1);
}static void kick_first_success_prefers_earliest_valid_offset_fixture(void) {
    ClearraBoard64Layout layout = standard_10x4();
    static const ClearraKickOffset clockwise_offsets[2] = {{0, 1}, {1, 1}};
    ClearraReachabilityKickTable table = {0};
    ClearraCandidateOperation operation;
    table.clockwise_offsets = clockwise_offsets;
    table.clockwise_count = 2;

    EXPECT_REACH_STATUS(clearra_kick_first_success(
                            layout, clearra_board64_empty(), CLEARRA_CANDIDATE_PIECE_T,
                            CLEARRA_CANDIDATE_ROTATION_ZERO,
                            CLEARRA_CANDIDATE_ROTATION_RIGHT, 0, 0, &table,
                            &operation),
                        CLEARRA_REACHABILITY_OK);

    EXPECT_U64(operation.kick_index, 0);
    EXPECT_U64(operation.x, 1);
    EXPECT_U64(operation.transition_kind, CLEARRA_ROTATION_TRANSITION_CLOCKWISE);
}static void reverse_kick_predecessor_uses_offset_inverse_fixture(void) {
    ClearraBoard64Layout layout = standard_10x4();
    ClearraReachabilityKickTable table = reverse_predecessor_kick_table();
    ClearraReachabilityReport report;
    uint64_t board = cell_mask(layout, 1, 3);

    EXPECT_REACH_STATUS(clearra_reachability_check(
                            layout, board, CLEARRA_CANDIDATE_PIECE_T,
                            CLEARRA_CANDIDATE_ROTATION_RIGHT, 1, 0,
                            CLEARRA_REACHABILITY_MODE_HARDDROP, &table, &report),
                        CLEARRA_REACHABILITY_OK);
    EXPECT_FALSE(report.reachable);

    EXPECT_REACH_STATUS(clearra_reachability_check(
                            layout, board, CLEARRA_CANDIDATE_PIECE_T,
                            CLEARRA_CANDIDATE_ROTATION_RIGHT, 1, 0,
                            CLEARRA_REACHABILITY_MODE_LOCKED, &table, &report),
                        CLEARRA_REACHABILITY_OK);
    EXPECT_TRUE(report.reachable);
    EXPECT_TRUE(report.used_kick);
    EXPECT_TRUE(report.path_complete);
    EXPECT_TRUE(report.has_rotation_evidence);
    EXPECT_TRUE(report.first_success_confirmed);
    EXPECT_U64(report.rotation_from, CLEARRA_CANDIDATE_ROTATION_ZERO);
    EXPECT_U64(report.rotation_to, CLEARRA_CANDIDATE_ROTATION_RIGHT);
    EXPECT_U64(report.rotation_request, CLEARRA_ROTATION_TRANSITION_CLOCKWISE);
    EXPECT_U64(report.kick_index, 0);
    EXPECT_U64((uint8_t)report.kick_dx, (uint8_t)-2);
    EXPECT_U64(report.kick_dy, 1);
    EXPECT_U64(report.predecessor_x, 2);
    EXPECT_U64(report.predecessor_y, 0);
    EXPECT_U64(report.result_x, 1);
    EXPECT_U64(report.result_y, 0);
}static void spawn_space_prevents_late_kick_false_accept(void) {
    ClearraBoard64Layout layout = standard_10x4();
    ClearraReachabilityKickTable table =
        srs_plus_kick_table(CLEARRA_CANDIDATE_PIECE_J);
    ClearraReachabilityReport report;

    EXPECT_REACH_STATUS(clearra_reachability_check(
                            layout, UINT64_C(0x3effbfe7),
                            CLEARRA_CANDIDATE_PIECE_J,
                            CLEARRA_CANDIDATE_ROTATION_LEFT, 3, 0,
                            CLEARRA_REACHABILITY_MODE_LOCKED_180, &table,
                            &report),
                        CLEARRA_REACHABILITY_OK);
    EXPECT_FALSE(report.reachable);
}static void spawn_space_preserves_pco_jtsz_source_path(void) {
    ClearraBoard64Layout layout = standard_10x4();
    ClearraReachabilityKickTable table =
        srs_plus_kick_table(CLEARRA_CANDIDATE_PIECE_S);
    ClearraReachabilityReport report;

    EXPECT_REACH_STATUS(clearra_reachability_check(
                            layout, UINT64_C(0x393f3fe7),
                            CLEARRA_CANDIDATE_PIECE_S,
                            CLEARRA_CANDIDATE_ROTATION_TWO, 4, 1,
                            CLEARRA_REACHABILITY_MODE_LOCKED_180, &table,
                            &report),
                        CLEARRA_REACHABILITY_OK);
    EXPECT_TRUE(report.reachable);
}static void harddrop_locked_180_and_kick_aware_modes_are_distinct(void) {
    EXPECT_FALSE(clearra_reachability_mode_uses_kicks(CLEARRA_REACHABILITY_MODE_HARDDROP));
    EXPECT_TRUE(clearra_reachability_mode_uses_kicks(CLEARRA_REACHABILITY_MODE_LOCKED));
    EXPECT_FALSE(clearra_reachability_mode_supports_180(CLEARRA_REACHABILITY_MODE_LOCKED));
    EXPECT_TRUE(clearra_reachability_mode_supports_180(CLEARRA_REACHABILITY_MODE_LOCKED_180));
    EXPECT_TRUE(clearra_reachability_mode_uses_kicks(CLEARRA_REACHABILITY_MODE_KICK_AWARE));
    EXPECT_U64(clearra_reachability_policy_for_mode(CLEARRA_REACHABILITY_MODE_HARDDROP),
               CLEARRA_REACHABILITY_POLICY_HARDDROP_ONLY);
    EXPECT_U64(clearra_reachability_policy_for_mode(CLEARRA_REACHABILITY_MODE_LOCKED),
               CLEARRA_REACHABILITY_POLICY_LOCKED_REVERSE_GRAPH);
    EXPECT_U64(clearra_reachability_policy_for_mode(CLEARRA_REACHABILITY_MODE_LOCKED_180),
               CLEARRA_REACHABILITY_POLICY_LOCKED_180_REVERSE_GRAPH);
}static void one_eighty_reachable_fixture(void) {
    ClearraBoard64Layout layout = standard_10x4();
    ClearraReachabilityKickTable table =
        srs_plus_kick_table(CLEARRA_CANDIDATE_PIECE_T);
    ClearraReachabilityReport report;
    uint64_t board = half_turn_only_board(layout);

    EXPECT_REACH_STATUS(clearra_reachability_check(
                            layout, board, CLEARRA_CANDIDATE_PIECE_T,
                            CLEARRA_CANDIDATE_ROTATION_TWO, 6, 0,
                            CLEARRA_REACHABILITY_MODE_LOCKED, &table, &report),
                        CLEARRA_REACHABILITY_OK);
    if (report.reachable) {
        print_reachability_debug_path(&report);
    }
    EXPECT_FALSE(report.reachable);
    EXPECT_FALSE(report.used_180);

    EXPECT_REACH_STATUS(clearra_reachability_check(
                            layout, board, CLEARRA_CANDIDATE_PIECE_T,
                            CLEARRA_CANDIDATE_ROTATION_TWO, 6, 0,
                            CLEARRA_REACHABILITY_MODE_LOCKED_180, &table, &report),
                        CLEARRA_REACHABILITY_OK);
    EXPECT_TRUE(report.reachable);
    EXPECT_TRUE(report.used_180);
    EXPECT_TRUE(report.debug_step_count > 1);
}static void locked180_mode_allows_half_turn_transition(void) {
    ClearraBoard64Layout layout = standard_10x4();
    ClearraReachabilityKickTable table = simple_kick_table();
    ClearraCandidateOperation operation;

    EXPECT_REACH_STATUS(clearra_kick_first_success(
                            layout, clearra_board64_empty(), CLEARRA_CANDIDATE_PIECE_T,
                            CLEARRA_CANDIDATE_ROTATION_ZERO,
                            CLEARRA_CANDIDATE_ROTATION_TWO, 1, 1, &table, &operation),
                        CLEARRA_REACHABILITY_OK);
    EXPECT_U64(operation.transition_kind, CLEARRA_ROTATION_TRANSITION_HALF_TURN);
}

static void reachable_lock_batch_matches_reverse_reference_on_board(
    uint64_t board,
    uint8_t mode) {
    ClearraBoard64Layout layout = standard_10x4();
    ClearraCompactRuleProfile profile = {0};
    EXPECT_TRUE(clearra_srs_plus_kick_table(&profile.kick_table) ==
                CLEARRA_RULE_OK);
    profile.rule_profile_id = CLR_RULE_SRS_PLUS;
    profile.kick_profile_id = CLR_KICK_SRS_PLUS_180;
    profile.supports_180 = true;
    ClearraReachabilityFrontier frontier;
    clearra_locked_reachability_frontier_init(&frontier);

    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        ClearraReachableLockSet locks;
        EXPECT_CANDIDATE_STATUS(
            clearra_reachable_lock_batch_generate(
                layout,
                board,
                piece,
                &profile,
                mode,
                &frontier,
                &locks),
            CLEARRA_CANDIDATE_OK);
        EXPECT_TRUE(locks.complete != 0u);

        ClearraReachabilityKickTable kick_table = {0};
        kick_table.compact_table = &profile.kick_table;
        kick_table.piece = piece;
        for (uint8_t rotation = 0u;
             rotation < CLEARRA_ROTATION_STATE_COUNT;
             ++rotation) {
            for (int8_t y = 0; y < (int8_t)layout.height; ++y) {
                for (int8_t x = 0; x < (int8_t)layout.width; ++x) {
                    ClearraReachabilityReport report;
                    ClearraReachabilityStatus status =
                        clearra_reachability_check(
                            layout,
                            board,
                            piece,
                            rotation,
                            x,
                            y,
                            mode == CLEARRA_CANDIDATE_MODE_LOCKED_180
                                ? CLEARRA_REACHABILITY_MODE_LOCKED_180
                                : CLEARRA_REACHABILITY_MODE_LOCKED,
                            &kick_table,
                            &report);
                    bool reference_reachable =
                        status == CLEARRA_REACHABILITY_OK && report.reachable;
                    if (status != CLEARRA_REACHABILITY_OK &&
                        status != CLEARRA_REACHABILITY_COLLISION &&
                        status != CLEARRA_REACHABILITY_UNREACHABLE) {
                        fprintf(
                            stderr,
                            "batch reference failed piece=%u rotation=%u x=%d y=%d status=%d\n",
                            (unsigned)piece,
                            (unsigned)rotation,
                            (int)x,
                            (int)y,
                            (int)status);
                        exit(1);
                    }
                    bool batch_reachable = clearra_reachable_lock_set_contains(
                        &locks, layout, rotation, x, y);
                    if (batch_reachable != reference_reachable) {
                        fprintf(
                            stderr,
                            "batch mismatch board=%llx mode=%u piece=%u rotation=%u x=%d y=%d expected=%u actual=%u\n",
                            (unsigned long long)board,
                            (unsigned)mode,
                            (unsigned)piece,
                            (unsigned)rotation,
                            (int)x,
                            (int)y,
                            (unsigned)reference_reachable,
                            (unsigned)batch_reachable);
                        exit(1);
                    }
                }
            }
        }
    }
}

static void reachable_lock_batch_matches_reverse_reference(void) {
    reachable_lock_batch_matches_reverse_reference_on_board(
        0u, CLEARRA_CANDIDATE_MODE_LOCKED_180);
    reachable_lock_batch_matches_reverse_reference_on_board(
        UINT64_C(0x393f3fe7), CLEARRA_CANDIDATE_MODE_LOCKED_180);
    reachable_lock_batch_matches_reverse_reference_on_board(
        UINT64_C(0x0000000000080401), CLEARRA_CANDIDATE_MODE_LOCKED);
}

int main(void) {
    collision_free_but_unreachable_fixture();
    harddrop_reachable_fixture();
    locked_reachability_rejects_ungrounded_collision_free_target();
    locked_reachable_via_multiple_movements_fixture();
    kick_reachable_only_with_first_success_offset_fixture();
    kick_order_mismatch_rejected_fixture();
    kick_first_success_prefers_earliest_valid_offset_fixture();
    reverse_kick_predecessor_uses_offset_inverse_fixture();
    spawn_space_prevents_late_kick_false_accept();
    spawn_space_preserves_pco_jtsz_source_path();
    harddrop_locked_180_and_kick_aware_modes_are_distinct();
    one_eighty_reachable_fixture();
    locked180_mode_allows_half_turn_transition();
    reachable_lock_batch_matches_reverse_reference();
    puts("core-c reachability tests passed");
    return 0;
}
