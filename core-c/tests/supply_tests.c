#include "clr_problem.h"
#include "clr_hold_automaton.h"
#include "clr_piece_source.h"

#include <stdio.h>
#include <stdlib.h>

#define EXPECT_TRUE(EXPR)                                                               \
    do {                                                                                \
        if (!(EXPR)) {                                                                  \
            fprintf(stderr, "%s:%d expected true\n", __FILE__, __LINE__);              \
            exit(1);                                                                    \
        }                                                                               \
    } while (0)

#define EXPECT_FALSE(EXPR)                                                              \
    do {                                                                                \
        if ((EXPR)) {                                                                   \
            fprintf(stderr, "%s:%d expected false\n", __FILE__, __LINE__);             \
            exit(1);                                                                    \
        }                                                                               \
    } while (0)

#define EXPECT_U32(EXPR, EXPECTED)                                                      \
    do {                                                                                \
        uint32_t actual_value = (uint32_t)(EXPR);                                       \
        uint32_t expected_value = (uint32_t)(EXPECTED);                                 \
        if (actual_value != expected_value) {                                           \
            fprintf(stderr, "%s:%d expected %u but got %u\n", __FILE__, __LINE__,       \
                    (unsigned)expected_value, (unsigned)actual_value);                  \
            exit(1);                                                                    \
        }                                                                               \
    } while (0)
static void fixed_sequence_passed_to_c_queue_view(void) {
    clr_queue_view view = clearra_queue_view_empty(
        CLR_QUEUE_FIXED_SEQUENCE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);
    view.len = 2;
    view.stored_len = 2;
    view.pieces[0] = CLR_PIECE_I;
    view.pieces[1] = CLR_PIECE_O;

    EXPECT_TRUE(clearra_queue_view_is_fixed_sequence(&view));
    EXPECT_TRUE(clearra_queue_view_preserves_provenance(
        &view,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE));
    EXPECT_U32(view.pieces[1], CLR_PIECE_O);
}static void bag_pattern_passed_to_c_queue_view(void) {
    clr_queue_view view = clearra_queue_view_empty(
        CLR_QUEUE_BAG_ALIGNED_PATTERN,
        CLR_SUPPLY_PROVENANCE_BAG_ALIGNED_PATTERN);
    clr_piece_window_descriptor piece_window =
        clearra_piece_window_descriptor(5, 5, true);
    view.len = 7;
    view.stored_len = 7;

    clr_bag_window bag = clearra_bag_window_from_queue_and_piece_window(
        &view,
        &piece_window);

    EXPECT_TRUE(clearra_queue_view_is_bag_pattern(&view));
    EXPECT_TRUE(clearra_bag_window_boundary_known(&bag));
    EXPECT_U32(bag.start, 0);
    EXPECT_U32(bag.len, 7);
}static void observed_expansion_remains_rust_owned(void) {
    clr_queue_view view = clearra_queue_view_empty(
        CLR_QUEUE_OBSERVED,
        CLR_SUPPLY_PROVENANCE_OBSERVED_RUST_EXPANDED);
    view.len = 3;
    view.stored_len = 3;

    EXPECT_TRUE(clearra_queue_view_is_observed_rust_expanded(&view));
    EXPECT_FALSE(clearra_queue_view_is_fixed_sequence(&view));
    EXPECT_TRUE(clearra_queue_view_preserves_provenance(
        &view,
        CLR_SUPPLY_PROVENANCE_OBSERVED_RUST_EXPANDED));
}static void supply_identity_preserves_provenance_cache_material(void) {
    clr_supply_identity_descriptor descriptor =
        clearra_supply_identity_descriptor(
            CLR_SUPPLY_PROVENANCE_OBSERVED_RUST_EXPANDED,
            CLR_SUPPLY_PROFILE_OBSERVED_STANDARD_7_BAG,
            CLR_PIECE_SET_STANDARD_TETROMINOES,
            42u,
            CLR_SUPPLY_BOUNDARY_OBSERVED_AMBIGUOUS,
            false,
            true);

    EXPECT_TRUE(clearra_supply_identity_descriptor_is_cache_key_material(
        &descriptor));
    EXPECT_U32(descriptor.supply_provenance_id,
               CLR_SUPPLY_PROVENANCE_OBSERVED_RUST_EXPANDED);
    EXPECT_U32(descriptor.bag_boundary_evidence,
               CLR_SUPPLY_BOUNDARY_OBSERVED_AMBIGUOUS);
    EXPECT_U32(descriptor.ambiguity_report, 1u);
    EXPECT_U32(descriptor.reserved, 0u);
}static void hold_state_passed_to_c(void) {
    clr_hold_state empty = clearra_hold_state_empty(1);
    clr_hold_state occupied = clearra_hold_state_occupied(CLR_PIECE_T);

    EXPECT_FALSE(clearra_hold_state_has_piece(&empty));
    EXPECT_TRUE(clearra_hold_state_has_piece(&occupied));
    EXPECT_U32(occupied.piece, CLR_PIECE_T);
}static void piece_window_descriptor_reports_exact_policy(void) {
    clr_piece_window_descriptor exact =
        clearra_piece_window_descriptor(6, 6, true);
    clr_piece_window_descriptor max_only =
        clearra_piece_window_descriptor(6, 0, false);

    EXPECT_TRUE(clearra_piece_window_has_exact(&exact));
    EXPECT_FALSE(clearra_piece_window_has_exact(&max_only));
}static void piece_source_descriptor_fixed_queue_roundtrip(void) {
    clr_piece_source_descriptor descriptor = {
        .piece_source_id = 99u,
        .source_kind = CLR_PIECE_SOURCE_FIXED_QUEUE,
        .provenance_id = CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE,
        .fixed_sequence_len = 2u,
        .piece_set_profile_id = CLR_PIECE_SET_STANDARD_TETROMINOES,
        .complete = 1u,
        .truncation_reason = CLR_SUPPLY_TRUNCATION_NONE,
    };

    EXPECT_TRUE(clearra_piece_source_descriptor_valid(&descriptor));
    EXPECT_TRUE(clearra_piece_source_descriptor_is_complete(&descriptor));
    EXPECT_TRUE(clearra_piece_source_descriptor_is_cache_material(&descriptor));
}static void piece_source_descriptor_materialized_universe_roundtrip(void) {
    clr_piece_source_descriptor descriptor = {
        .piece_source_id = 100u,
        .source_kind = CLR_PIECE_SOURCE_MATERIALIZED_PATTERN_UNIVERSE,
        .provenance_id = CLR_SUPPLY_PROVENANCE_BAG_ALIGNED_PATTERN,
        .pattern_universe_id = 42u,
        .pattern_weight_model_id = 7u,
        .materialized_pattern_count = 5040u,
        .piece_set_profile_id = CLR_PIECE_SET_STANDARD_TETROMINOES,
        .complete = 0u,
        .truncation_reason =
            CLR_SUPPLY_TRUNCATION_MATERIALIZED_PATTERN_BUDGET_EXCEEDED,
    };

    EXPECT_TRUE(clearra_piece_source_descriptor_valid(&descriptor));
    EXPECT_FALSE(clearra_piece_source_descriptor_is_complete(&descriptor));
}static void hold_automaton_uses_current_piece(void) {
    clr_hold_automaton_state state = {
        11u, 3u, 2u, 0xfeedu, 77u, CLR_PIECE_NONE, 1u,
        0u, 0u, {0u, 0u, 0u, 0u}};
    clr_hold_automaton_step step;

    uint32_t status = clearra_hold_automaton_apply(
        &state,
        CLR_HOLD_TRANSITION_USE_CURRENT,
        CLR_PIECE_I,
        CLR_PIECE_NONE,
        &step);

    EXPECT_U32(status, CLR_HOLD_AUTOMATON_OK);
    EXPECT_U32(step.used_piece, CLR_PIECE_I);
    EXPECT_U32(step.next_state.cursor, 4u);
    EXPECT_U32(step.next_state.hold_empty, 1u);
}static void hold_automaton_swaps_held_piece(void) {
    clr_hold_automaton_state state = {
        11u, 3u, 2u, 0xfeedu, 77u, CLR_PIECE_T, 0u,
        0u, 0u, {0u, 0u, 0u, 0u}};
    clr_hold_automaton_step step;

    uint32_t status = clearra_hold_automaton_apply(
        &state,
        CLR_HOLD_TRANSITION_SWAP_HELD,
        CLR_PIECE_I,
        CLR_PIECE_NONE,
        &step);

    EXPECT_U32(status, CLR_HOLD_AUTOMATON_OK);
    EXPECT_U32(step.used_piece, CLR_PIECE_T);
    EXPECT_U32(step.next_state.cursor, 4u);
    EXPECT_U32(step.next_state.hold_piece, CLR_PIECE_I);
}static void hold_automaton_stores_current_then_uses_next(void) {
    clr_hold_automaton_state state = {
        11u, 3u, 2u, 0xfeedu, 77u, CLR_PIECE_NONE, 1u,
        0u, 0u, {0u, 0u, 0u, 0u}};
    clr_hold_automaton_step step;

    uint32_t status = clearra_hold_automaton_apply(
        &state,
        CLR_HOLD_TRANSITION_STORE_CURRENT_THEN_USE_NEXT,
        CLR_PIECE_I,
        CLR_PIECE_O,
        &step);

    EXPECT_U32(status, CLR_HOLD_AUTOMATON_OK);
    EXPECT_U32(step.used_piece, CLR_PIECE_O);
    EXPECT_U32(step.next_state.cursor, 5u);
    EXPECT_U32(step.next_state.hold_piece, CLR_PIECE_I);
}static void hold_automaton_preserves_long_carryover(void) {
    clr_hold_automaton_state state = {
        11u, 3u, 9u, 0xabcu, 77u, CLR_PIECE_L, 0u,
        0u, 0u, {0u, 0u, 0u, 0u}};
    clr_hold_automaton_step step;

    uint32_t status = clearra_hold_automaton_apply(
        &state,
        CLR_HOLD_TRANSITION_USE_CURRENT,
        CLR_PIECE_S,
        CLR_PIECE_Z,
        &step);

    EXPECT_U32(status, CLR_HOLD_AUTOMATON_OK);
    EXPECT_U32(step.next_state.hold_piece, CLR_PIECE_L);
    EXPECT_U32(step.next_state.bag_epoch, 9u);
    EXPECT_U32((uint32_t)step.next_state.bag_remainder_key, 0xabcu);
}static void hold_automaton_state_in_buildup_memo_key(void) {
    clr_hold_automaton_state state = {
        11u, 3u, 2u, 0xfeedu, 77u, CLR_PIECE_J, 0u,
        1u, 1u, {0u, 0u, 0u, 0u}};

    clr_buildup_hold_automaton_memo_key key =
        clearra_buildup_hold_automaton_memo_key(&state);

    EXPECT_U32((uint32_t)key.piece_source_id, 11u);
    EXPECT_U32(key.cursor, 3u);
    EXPECT_U32(key.bag_epoch, 2u);
    EXPECT_U32((uint32_t)key.bag_remainder_key, 0xfeedu);
    EXPECT_U32((uint32_t)key.provenance_id, 77u);
    EXPECT_U32(key.hold_piece, CLR_PIECE_J);
    EXPECT_U32(key.terminal_projection_consumed, 1u);
    EXPECT_U32(key.terminal_projection_provenance, 1u);
    uint64_t terminal_hash =
        clearra_buildup_hold_automaton_memo_key_hash(&key);
    EXPECT_TRUE(terminal_hash != 0u);
    key.terminal_projection_consumed = 0u;
    key.terminal_projection_provenance = 0u;
    EXPECT_TRUE(clearra_buildup_hold_automaton_memo_key_hash(&key) !=
                terminal_hash);
}int main(void) {
    fixed_sequence_passed_to_c_queue_view();
    bag_pattern_passed_to_c_queue_view();
    observed_expansion_remains_rust_owned();
    supply_identity_preserves_provenance_cache_material();
    hold_state_passed_to_c();
    piece_window_descriptor_reports_exact_policy();
    piece_source_descriptor_fixed_queue_roundtrip();
    piece_source_descriptor_materialized_universe_roundtrip();
    hold_automaton_uses_current_piece();
    hold_automaton_swaps_held_piece();
    hold_automaton_stores_current_then_uses_next();
    hold_automaton_preserves_long_carryover();
    hold_automaton_state_in_buildup_memo_key();
    puts("core-c supply tests passed");
    return 0;
}
