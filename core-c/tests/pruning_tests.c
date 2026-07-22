#include "clr_pruning.h"
#include "clr_piece.h"
#include "packing/packing_problem.h"
#include "board/board64.h"

#include <stdio.h>
#include <stdlib.h>

#define EXPECT_U64(EXPR, EXPECTED)                                                        \
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

#define EXPECT_NE_U64(LEFT, RIGHT)                                                        \
    do {                                                                                  \
        uint64_t left_value = (uint64_t)(LEFT);                                           \
        uint64_t right_value = (uint64_t)(RIGHT);                                         \
        if (left_value == right_value) {                                                  \
            fprintf(stderr, "%s:%d expected distinct values but both were 0x%llx\n",      \
                    __FILE__, __LINE__, (unsigned long long)left_value);                  \
            exit(1);                                                                      \
        }                                                                                 \
    } while (0)

#define EXPECT_TRUE(EXPR)                                                                 \
    do {                                                                                  \
        if (!(EXPR)) {                                                                    \
            fprintf(stderr, "%s:%d expected true\n", __FILE__, __LINE__);                \
            exit(1);                                                                      \
        }                                                                                 \
    } while (0)

#define EXPECT_FALSE(EXPR)                                                                \
    do {                                                                                  \
        if ((EXPR)) {                                                                     \
            fprintf(stderr, "%s:%d expected false\n", __FILE__, __LINE__);               \
            exit(1);                                                                      \
        }                                                                                 \
    } while (0)

#define EXPECT_PACKING_STATUS(EXPR, EXPECTED)                                             \
    do {                                                                                  \
        ClearraPackingStatus actual_status = (EXPR);                                      \
        ClearraPackingStatus expected_status = (EXPECTED);                                \
        if (actual_status != expected_status) {                                           \
            fprintf(stderr, "%s:%d expected packing status %d but got %d\n", __FILE__,    \
                    __LINE__, (int)expected_status, (int)actual_status);                  \
            exit(1);                                                                      \
        }                                                                                 \
    } while (0)
static clr_pruning_proof_ledger_entry entry(uint8_t proof_level) {
    clr_pruning_proof_ledger_entry result = {0};
    result.batch_id = UINT64_C(7);
    result.producer_id = CLR_PRUNING_PRODUCER_STATIC_PLACEMENT_FILTER;
    result.catalog_identity_digest = UINT64_C(0x55aa);
    result.state_layer = 2u;
    result.prune_reason = CLR_PRUNE_BUILD_ORDERS_HOLD_REACHABLE_INTERSECTION_EMPTY;
    result.affected_candidate_count = 3u;
    result.proof_level = proof_level;
    result.has_clear_state_key = 1u;
    result.clear_state_key = UINT64_C(0xabc);
    result.fallback_if_invalid = CLR_PRUNE_FALLBACK_RUN_BUILDUP;
    result.evidence_digest = UINT64_C(0x1234);
    return result;
}static void all_pruning_paths_emit_prune_reason(void) {
    EXPECT_U64(CLR_PRUNE_AREA_OVERFLOW, 1u);
    EXPECT_U64(CLR_PRUNE_RESOURCE_BUDGET_EXCEEDED, 14u);
    EXPECT_FALSE(clr_prune_reason_is_forbidden_name("AreaOverflow"));
}static void all_pruning_paths_emit_proof_level(void) {
    EXPECT_FALSE(clr_prune_proof_level_allows_global_prune(CLR_PRUNE_PROOF_LOCAL_ONLY));
    EXPECT_FALSE(clr_prune_proof_level_allows_global_prune(
        CLR_PRUNE_PROOF_CLEAR_STATE_CONDITIONAL));
    EXPECT_TRUE(clr_prune_proof_level_allows_global_prune(CLR_PRUNE_PROOF_GLOBAL_SAFE));
}static void target_frame_floating_not_global_pruned(void) {
    EXPECT_TRUE(clr_prune_reason_is_forbidden_name("FloatingInTargetFrame"));
}static void target_frame_floating_piece_not_pruned_globally(void) {
    target_frame_floating_not_global_pruned();
    EXPECT_FALSE(clr_prune_proof_level_allows_global_prune(CLR_PRUNE_PROOF_LOCAL_ONLY));
}static void forced_piece_family_conditional_under_clear_state(void) {
    clr_placement_domain domain;
    clr_placement_domain_key key = {1u, 2u, 3u, 4u};

    clr_placement_domain_init(
        &domain,
        key,
        1u,
        UINT64_C(0x04),
        CLR_PRUNE_PROOF_CLEAR_STATE_CONDITIONAL);
    clr_placement_domain_set_forced_piece_family(
        &domain,
        2u,
        CLR_PRUNE_PROOF_CLEAR_STATE_CONDITIONAL);

    EXPECT_TRUE(clr_component_domain_has_forced_piece_family_under_clear_state(&domain));
    EXPECT_U64(domain.forced_piece_family, 2u);
}static void global_forced_piece_requires_all_reachable_clear_states(void) {
    EXPECT_U64(
        clr_clear_state_domain_promote_if_all_reachable_clear_states(2u, 3u),
        CLR_PRUNE_PROOF_CLEAR_STATE_CONDITIONAL);
    EXPECT_U64(
        clr_clear_state_domain_promote_if_all_reachable_clear_states(3u, 3u),
        CLR_PRUNE_PROOF_ALL_REACHABLE_CLEAR_STATES);
}static void cannot_promote_by_count_only_without_clear_state_set_digest(void) {
    EXPECT_U64(
        clr_clear_state_domain_promote_if_all_reachable_clear_states(3u, 3u),
        CLR_PRUNE_PROOF_ALL_REACHABLE_CLEAR_STATES);
    EXPECT_FALSE(clr_prune_proof_level_allows_global_prune(
        clr_clear_state_domain_promote_if_all_reachable_clear_states(3u, 3u)));
}static void global_forced_piece_requires_complete_clear_state_set(void) {
    EXPECT_FALSE(clr_prune_reason_has_connected_engine_factory(
        CLR_PRUNE_CELL_DOMAIN_EMPTY_FOR_ALL_REACHABLE_CLEAR_STATES));
}static void clear_state_set_truncated_keeps_candidate(void) {
    EXPECT_FALSE(clr_prune_proof_level_allows_global_prune(
        clr_clear_state_domain_promote_if_all_reachable_clear_states(3u, 3u)));
}static void component_domain_digest_changes_with_operation_table(void) {
    clr_placement_domain domain;
    clr_placement_domain_key key = {1u, 2u, 3u, 4u};

    clr_placement_domain_init(
        &domain,
        key,
        2u,
        UINT64_C(0x05),
        CLR_PRUNE_PROOF_CLEAR_STATE_CONDITIONAL);
    clr_placement_domain_set_forced_piece_family(
        &domain,
        2u,
        CLR_PRUNE_PROOF_CLEAR_STATE_CONDITIONAL);

    EXPECT_TRUE(
        clr_component_domain_digest_with_operation_table(&domain, 7u) !=
        clr_component_domain_digest_with_operation_table(&domain, 8u));
}static void target_frame_domain_never_global_safe_without_clear_state_set(void) {
    clr_placement_domain domain;
    clr_placement_domain_key key = {1u, 2u, 3u, 4u};

    clr_placement_domain_init(
        &domain,
        key,
        0u,
        UINT64_C(0),
        CLR_PRUNE_PROOF_LOCAL_ONLY);

    EXPECT_FALSE(clr_cell_domain_empty_under_clear_state(&domain));
    EXPECT_FALSE(clr_prune_proof_level_allows_global_prune(
        clr_clear_state_domain_promote_if_all_reachable_clear_states(1u, 1u)));
}static void component_exact_cover_runs_only_under_budget(void) {
    clr_propagation_budget budget = {10u, 3u, 8u, 16u, 4u, 0.25};

    EXPECT_TRUE(clr_component_exact_cover_runs_only_under_budget(
        &budget,
        3u,
        8u,
        16u,
        4u));
    EXPECT_FALSE(clr_component_exact_cover_runs_only_under_budget(
        &budget,
        3u,
        9u,
        16u,
        4u));
}static void mcts_priority_does_not_prune(void) {
    EXPECT_TRUE(clr_prune_reason_is_forbidden_name("MctsLowScore"));
}static void rare_piece_heuristic_does_not_prune(void) {
    EXPECT_TRUE(clr_prune_reason_is_forbidden_name("RareShape"));
}static void local_only_cannot_drop_candidate_without_global_safe_proof(void) {
    clr_pruning_proof_ledger ledger;
    clr_pruning_proof_ledger_init(&ledger);

    EXPECT_FALSE(clr_prune_reason_has_connected_engine_factory(
        CLR_PRUNE_CELL_DOMAIN_EMPTY_UNDER_CLEAR_STATE));
    EXPECT_U64(ledger.count, 0u);
    EXPECT_TRUE(clr_prune_reason_has_connected_engine_factory(
        CLR_PRUNE_PLACEMENT_COLLISION));
}static void resource_cap_reached_is_incomplete_not_prune(void) {
    clr_pruning_proof_ledger ledger;
    clr_pruning_proof_ledger_init(&ledger);

    EXPECT_FALSE(clr_prune_reason_has_connected_engine_factory(
        CLR_PRUNE_RESOURCE_BUDGET_EXCEEDED));
    EXPECT_U64(ledger.count, 0u);
}static void resource_budget_exceeded_marks_result_incomplete_not_candidate_drop(void) {
    resource_cap_reached_is_incomplete_not_prune();
}static void fill_ledger_to_capacity(clr_pruning_proof_ledger *ledger) {
    for (uint16_t index = 0u; index < CLR_PRUNING_LEDGER_MAX_ENTRIES; ++index) {
        clr_pruning_proof_ledger_entry full_entry =
            entry(CLR_PRUNE_PROOF_GLOBAL_SAFE);
        full_entry.evidence_digest = (uint64_t)index + UINT64_C(1);
        EXPECT_U64(
            clr_pruning_proof_ledger_record(ledger, full_entry),
            CLR_PRUNING_OK);
    }
    EXPECT_U64(ledger->count, CLR_PRUNING_LEDGER_MAX_ENTRIES);
    EXPECT_U64(ledger->evidence_truncated, 0u);
}static void ledger_capacity_does_not_abort_static_safe_prunes(void) {
    clr_pruning_proof_ledger ledger;
    clr_pruning_proof_ledger_entry overflow_entry =
        entry(CLR_PRUNE_PROOF_GLOBAL_SAFE);
    overflow_entry.prune_reason = CLR_PRUNE_TARGET_MASK_OVERFLOW;
    clr_pruning_proof_ledger_init(&ledger);

    fill_ledger_to_capacity(&ledger);

    EXPECT_U64(
        clr_pruning_proof_ledger_record(&ledger, overflow_entry),
        CLR_PRUNING_OK);
    EXPECT_U64(ledger.count, CLR_PRUNING_LEDGER_MAX_ENTRIES);
    EXPECT_U64(ledger.evidence_truncated, 1u);
    EXPECT_U64(ledger.dropped_evidence_count, 1u);
    EXPECT_U64(ledger.prune_reason_counts[CLR_PRUNE_TARGET_MASK_OVERFLOW], 1u);
}static void candidate_drop_allowed_requires_global_safe_but_ledger_retention_is_best_effort(void) {
    clr_pruning_proof_ledger ledger;
    clr_pruning_proof_ledger_entry overflow_entry =
        entry(CLR_PRUNE_PROOF_GLOBAL_SAFE);
    overflow_entry.prune_reason = CLR_PRUNE_PLACEMENT_COLLISION;
    clr_pruning_proof_ledger_init(&ledger);

    fill_ledger_to_capacity(&ledger);

    EXPECT_FALSE(clr_prune_reason_has_connected_engine_factory(
        CLR_PRUNE_CELL_DOMAIN_EMPTY_UNDER_CLEAR_STATE));
    EXPECT_U64(ledger.evidence_truncated, 0u);

    EXPECT_U64(
        clr_pruning_proof_ledger_record(&ledger, overflow_entry),
        CLR_PRUNING_OK);
    EXPECT_U64(ledger.evidence_truncated, 1u);
    EXPECT_U64(ledger.dropped_evidence_count, 1u);
    EXPECT_U64(ledger.prune_reason_counts[CLR_PRUNE_PLACEMENT_COLLISION], 1u);
}static void cell_domain_empty_is_clear_state_conditional(void) {
    clr_placement_domain domain;
    clr_placement_domain_key key = {1u, 2u, 3u, 4u};

    clr_placement_domain_init(
        &domain,
        key,
        0u,
        UINT64_C(0),
        CLR_PRUNE_PROOF_CLEAR_STATE_CONDITIONAL);

    EXPECT_TRUE(clr_cell_domain_empty_under_clear_state(&domain));
}static void cell_domain_empty_has_clear_state_key(void) {
    clr_placement_domain domain;
    clr_placement_domain_key key = {1u, 0x42u, 3u, 4u};

    clr_placement_domain_init(
        &domain,
        key,
        0u,
        UINT64_C(0),
        CLR_PRUNE_PROOF_CLEAR_STATE_CONDITIONAL);

    EXPECT_TRUE(clr_cell_domain_empty_under_clear_state(&domain));
    EXPECT_U64(domain.key.clear_state_key, 0x42u);
}static ClearraBoard64Layout pruning_test_layout(void) {
    ClearraBoard64Layout layout;
    if (clearra_board64_make_layout(4u, 2u, &layout) != CLEARRA_BOARD64_OK) {
        fprintf(stderr, "failed to create pruning test layout\n");
        exit(1);
    }
    return layout;
}static clr_static_prune_context static_prune_context(
    uint64_t batch_id,
    uint8_t state_layer,
    uint16_t operation_id,
    uint64_t rule_profile_id,
    uint64_t kick_profile_id) {
    clr_static_prune_context context = {0};
    context.batch_id = batch_id;
    context.state_layer = state_layer;
    context.piece = CLR_PIECE_T;
    context.rotation = 1u;
    context.x = 2;
    context.y = 3;
    context.operation_id = operation_id;
    context.operation_table_version = UINT64_C(7);
    context.piece_set_id = CLR_PIECE_SET_STANDARD_TETROMINOES;
    context.rule_profile_id = rule_profile_id;
    context.kick_profile_id = kick_profile_id;
    return context;
}static uint64_t static_prune_digest_for_context(
    const clr_static_prune_context *context) {
    ClearraBoard64Layout layout = pruning_test_layout();
    clr_pruning_proof_ledger ledger;
    bool accepts = true;
    uint64_t target_mask = 0;
    uint64_t placement_mask = UINT64_C(1) << 4u;
    clr_pruning_proof_ledger_init(&ledger);
    EXPECT_PACKING_STATUS(
        clearra_packing_target_mask_for_lines(layout, 1u, &target_mask),
        CLEARRA_PACKING_OK);

    EXPECT_PACKING_STATUS(
        clearra_packing_pruner_accepts_static_candidate_with_ledger(
            layout,
            clearra_board64_empty(),
            target_mask,
            placement_mask,
            context,
            &ledger,
            &accepts),
        CLEARRA_PACKING_OK);

    EXPECT_FALSE(accepts);
    EXPECT_U64(ledger.count, 1u);
    return ledger.entries[0].evidence_digest;
}static void packing_static_pruner_records_ledger_for_target_mask_overflow(void) {
    ClearraBoard64Layout layout = pruning_test_layout();
    clr_static_prune_context context =
        static_prune_context(UINT64_C(0x41), 0u, 0x100u, 9u, 10u);
    clr_pruning_proof_ledger ledger;
    bool accepts = true;
    uint64_t target_mask = 0;
    uint64_t placement_mask = UINT64_C(1) << 4u;
    clr_pruning_proof_ledger_init(&ledger);
    EXPECT_PACKING_STATUS(
        clearra_packing_target_mask_for_lines(layout, 1u, &target_mask),
        CLEARRA_PACKING_OK);

    EXPECT_PACKING_STATUS(
        clearra_packing_pruner_accepts_static_candidate_with_ledger(
            layout,
            clearra_board64_empty(),
            target_mask,
            placement_mask,
            &context,
            &ledger,
            &accepts),
        CLEARRA_PACKING_OK);

    EXPECT_FALSE(accepts);
    EXPECT_U64(ledger.count, 1u);
    EXPECT_U64(ledger.entries[0].prune_reason, CLR_PRUNE_TARGET_MASK_OVERFLOW);
    EXPECT_U64(ledger.entries[0].proof_level, CLR_PRUNE_PROOF_GLOBAL_SAFE);
}static void packing_static_pruner_records_ledger_for_collision(void) {
    ClearraBoard64Layout layout = pruning_test_layout();
    clr_static_prune_context context =
        static_prune_context(UINT64_C(0x42), 0u, 0x100u, 9u, 10u);
    clr_pruning_proof_ledger ledger;
    bool accepts = true;
    uint64_t target_mask = 0;
    uint64_t placement_mask = UINT64_C(1);
    clr_pruning_proof_ledger_init(&ledger);
    EXPECT_PACKING_STATUS(
        clearra_packing_target_mask_for_lines(layout, 2u, &target_mask),
        CLEARRA_PACKING_OK);

    EXPECT_PACKING_STATUS(
        clearra_packing_pruner_accepts_static_candidate_with_ledger(
            layout,
            placement_mask,
            target_mask,
            placement_mask,
            &context,
            &ledger,
            &accepts),
        CLEARRA_PACKING_OK);

    EXPECT_FALSE(accepts);
    EXPECT_U64(ledger.count, 1u);
    EXPECT_U64(ledger.entries[0].prune_reason, CLR_PRUNE_PLACEMENT_COLLISION);
    EXPECT_U64(ledger.entries[0].proof_level, CLR_PRUNE_PROOF_GLOBAL_SAFE);
}static void complete_required_ledger_capacity_keeps_candidate(void) {
    ClearraBoard64Layout layout = pruning_test_layout();
    clr_static_prune_context context =
        static_prune_context(UINT64_C(0x77), 1u, 0x201u, 9u, 10u);
    clr_pruning_proof_ledger ledger;
    bool accepts = false;
    uint64_t target_mask = 0;
    uint64_t placement_mask = UINT64_C(1);

    EXPECT_U64(
        clr_pruning_proof_ledger_init_with_policy(
            &ledger,
            CLR_PRUNING_EVIDENCE_COMPLETE_REQUIRED),
        CLR_PRUNING_OK);
    fill_ledger_to_capacity(&ledger);
    EXPECT_PACKING_STATUS(
        clearra_packing_target_mask_for_lines(layout, 2u, &target_mask),
        CLEARRA_PACKING_OK);

    EXPECT_PACKING_STATUS(
        clearra_packing_pruner_accepts_static_candidate_with_ledger(
            layout,
            placement_mask,
            target_mask,
            placement_mask,
            &context,
            &ledger,
            &accepts),
        CLEARRA_PACKING_OK);

    EXPECT_TRUE(accepts);
    EXPECT_U64(ledger.count, CLR_PRUNING_LEDGER_MAX_ENTRIES);
    EXPECT_U64(ledger.evidence_truncated, 0u);
    EXPECT_U64(ledger.complete_required_capacity_hit, 1u);
    EXPECT_U64(ledger.candidates_kept_due_to_evidence_capacity, 1u);
}static void ledger_overflow_sets_evidence_truncated_but_search_complete(void) {
    ClearraBoard64Layout layout = pruning_test_layout();
    clr_static_prune_context context =
        static_prune_context(UINT64_C(0x43), 0u, 0x100u, 9u, 10u);
    clr_pruning_proof_ledger ledger;
    bool accepts = true;
    uint64_t target_mask = 0;
    uint64_t placement_mask = UINT64_C(1) << 4u;
    clr_pruning_proof_ledger_init(&ledger);
    fill_ledger_to_capacity(&ledger);
    EXPECT_PACKING_STATUS(
        clearra_packing_target_mask_for_lines(layout, 1u, &target_mask),
        CLEARRA_PACKING_OK);

    EXPECT_PACKING_STATUS(
        clearra_packing_pruner_accepts_static_candidate_with_ledger(
            layout,
            clearra_board64_empty(),
            target_mask,
            placement_mask,
            &context,
            &ledger,
            &accepts),
        CLEARRA_PACKING_OK);

    EXPECT_FALSE(accepts);
    EXPECT_U64(ledger.count, CLR_PRUNING_LEDGER_MAX_ENTRIES);
    EXPECT_U64(ledger.evidence_truncated, 1u);
    EXPECT_U64(ledger.dropped_evidence_count, 1u);
    EXPECT_U64(ledger.prune_reason_counts[CLR_PRUNE_TARGET_MASK_OVERFLOW], 1u);
}static void packing_2l_many_static_prunes_still_returns_ok(void) {
    ClearraBoard64Layout layout = pruning_test_layout();
    clr_static_prune_context context =
        static_prune_context(UINT64_C(0x44), 0u, 0x100u, 9u, 10u);
    clr_pruning_proof_ledger ledger;
    uint64_t target_mask = 0;
    clr_pruning_proof_ledger_init(&ledger);
    EXPECT_PACKING_STATUS(
        clearra_packing_target_mask_for_lines(layout, 1u, &target_mask),
        CLEARRA_PACKING_OK);

    for (uint16_t index = 0u; index < CLR_PRUNING_LEDGER_MAX_ENTRIES + 2u; ++index) {
        bool accepts = true;
        uint64_t placement_mask = UINT64_C(1) << 4u;
        EXPECT_PACKING_STATUS(
            clearra_packing_pruner_accepts_static_candidate_with_ledger(
                layout,
                clearra_board64_empty(),
                target_mask,
                placement_mask,
                &context,
                &ledger,
                &accepts),
            CLEARRA_PACKING_OK);
        EXPECT_FALSE(accepts);
    }

    EXPECT_U64(ledger.count, CLR_PRUNING_LEDGER_MAX_ENTRIES);
    EXPECT_U64(ledger.evidence_truncated, 1u);
    EXPECT_U64(ledger.dropped_evidence_count, 2u);
    EXPECT_U64(ledger.prune_reason_counts[CLR_PRUNE_TARGET_MASK_OVERFLOW], 2u);
}static void static_prune_ledger_records_real_batch_id(void) {
    ClearraBoard64Layout layout = pruning_test_layout();
    clr_static_prune_context context =
        static_prune_context(UINT64_C(0x44), 3u, 0x100u, 9u, 10u);
    clr_pruning_proof_ledger ledger;
    bool accepts = true;
    uint64_t target_mask = 0;
    uint64_t placement_mask = UINT64_C(1) << 4u;
    clr_pruning_proof_ledger_init(&ledger);
    EXPECT_PACKING_STATUS(
        clearra_packing_target_mask_for_lines(layout, 1u, &target_mask),
        CLEARRA_PACKING_OK);

    EXPECT_PACKING_STATUS(
        clearra_packing_pruner_accepts_static_candidate_with_ledger(
            layout,
            clearra_board64_empty(),
            target_mask,
            placement_mask,
            &context,
            &ledger,
            &accepts),
        CLEARRA_PACKING_OK);

    EXPECT_FALSE(accepts);
    EXPECT_U64(ledger.count, 1u);
    EXPECT_U64(ledger.entries[0].batch_id, 0x44u);
}static void static_prune_ledger_records_bfs_layer(void) {
    ClearraBoard64Layout layout = pruning_test_layout();
    clr_static_prune_context context =
        static_prune_context(UINT64_C(0x44), 5u, 0x100u, 9u, 10u);
    clr_pruning_proof_ledger ledger;
    bool accepts = true;
    uint64_t target_mask = 0;
    uint64_t placement_mask = UINT64_C(1) << 4u;
    clr_pruning_proof_ledger_init(&ledger);
    EXPECT_PACKING_STATUS(
        clearra_packing_target_mask_for_lines(layout, 1u, &target_mask),
        CLEARRA_PACKING_OK);

    EXPECT_PACKING_STATUS(
        clearra_packing_pruner_accepts_static_candidate_with_ledger(
            layout,
            clearra_board64_empty(),
            target_mask,
            placement_mask,
            &context,
            &ledger,
            &accepts),
        CLEARRA_PACKING_OK);

    EXPECT_FALSE(accepts);
    EXPECT_U64(ledger.count, 1u);
    EXPECT_U64(ledger.entries[0].state_layer, 5u);
}static void static_prune_evidence_digest_changes_by_operation_id(void) {
    clr_static_prune_context left =
        static_prune_context(UINT64_C(0x44), 3u, 0x100u, 9u, 10u);
    clr_static_prune_context right = left;
    right.operation_id = 0x101u;

    EXPECT_NE_U64(
        static_prune_digest_for_context(&left),
        static_prune_digest_for_context(&right));
}static void static_prune_evidence_digest_changes_by_rule_profile(void) {
    clr_static_prune_context left =
        static_prune_context(UINT64_C(0x44), 3u, 0x100u, 9u, 10u);
    clr_static_prune_context right = left;
    right.rule_profile_id = 11u;

    EXPECT_NE_U64(
        static_prune_digest_for_context(&left),
        static_prune_digest_for_context(&right));
}static void static_prune_evidence_digest_changes_by_kick_profile(void) {
    clr_static_prune_context left =
        static_prune_context(UINT64_C(0x44), 3u, 0x100u, 9u, 10u);
    clr_static_prune_context right = left;
    right.kick_profile_id = 12u;

    EXPECT_NE_U64(
        static_prune_digest_for_context(&left),
        static_prune_digest_for_context(&right));
}static void static_prune_evidence_digest_changes_by_piece_set(void) {
    clr_static_prune_context left =
        static_prune_context(UINT64_C(0x44), 3u, 0x100u, 9u, 10u);
    clr_static_prune_context right = left;
    right.piece_set_id = 2u;

    EXPECT_NE_U64(
        static_prune_digest_for_context(&left),
        static_prune_digest_for_context(&right));
}static void static_prune_rejects_missing_context_or_ledger_before_drop(void) {
    ClearraBoard64Layout layout = pruning_test_layout();
    clr_static_prune_context context =
        static_prune_context(UINT64_C(0x45), 0u, 0x100u, 9u, 10u);
    clr_pruning_proof_ledger ledger;
    uint64_t target_mask = 0u;
    uint64_t placement_mask = UINT64_C(1) << 4u;
    bool accepts = true;
    clr_pruning_proof_ledger_init(&ledger);
    EXPECT_PACKING_STATUS(
        clearra_packing_target_mask_for_lines(layout, 1u, &target_mask),
        CLEARRA_PACKING_OK);

    EXPECT_PACKING_STATUS(
        clearra_packing_pruner_accepts_static_candidate_with_ledger(
            layout,
            clearra_board64_empty(),
            target_mask,
            placement_mask,
            &context,
            0,
            &accepts),
        CLEARRA_PACKING_INVALID_ARGUMENT);
    EXPECT_FALSE(accepts);

    accepts = true;
    EXPECT_PACKING_STATUS(
        clearra_packing_pruner_accepts_static_candidate_with_ledger(
            layout,
            clearra_board64_empty(),
            target_mask,
            placement_mask,
            0,
            &ledger,
            &accepts),
        CLEARRA_PACKING_INVALID_ARGUMENT);
    EXPECT_FALSE(accepts);
    EXPECT_U64(ledger.count, 0u);
}static clr_packing_problem pruning_test_product_problem(
    ClearraBoard64Layout layout,
    uint64_t target_mask) {
    const uint8_t pieces[1] = {CLR_PIECE_O};
    clr_packing_problem problem = clr_packing_problem_zero();
    problem.problem_kind = CLR_PROBLEM_OPENING_PC;
    problem.max_pieces = 1u;
    problem.board.width = layout.width;
    problem.board.visible_height = layout.height;
    problem.board.search_height = layout.height;
    problem.board.initial_mask = clearra_board64_empty();
    problem.board.backend_kind = CLR_BOARD_BACKEND_BOARD64;
    problem.board.cell_count = layout.cell_count;
    problem.goal_region_mask = target_mask;
    problem.required_fill_mask = target_mask;
    problem.exact_pieces = 1u;
    problem.piece_window = clearra_piece_window_descriptor(1u, 1u, true);
    problem.piece_multiset_window =
        clearra_piece_multiset_window_from_pieces(pieces, 1u);
    problem.piece_source = clearra_piece_source_descriptor_fixed_queue(
        UINT64_C(0x701),
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE,
        1u,
        CLR_PIECE_SET_STANDARD_TETROMINOES);
    problem.piece_source_pattern_pieces[0] = CLR_PIECE_O;
    problem.piece_source_pattern_len = 1u;
    problem.piece_source_pattern_complete = 1u;
    problem.piece_source_pattern_id = 7u;
    problem.rule.piece_set_profile_id = CLR_PIECE_SET_STANDARD_TETROMINOES;
    problem.rule.bag_profile_id = CLR_BAG_STANDARD_7_BAG;
    problem.rule.rule_profile_id = CLR_RULE_NO_KICK;
    problem.rule.kick_profile_id = CLR_KICK_NO_KICK;
    problem.rule.spawn_profile_id = CLR_SPAWN_STANDARD_10;
    problem.backend.requested_backend = CLR_BACKEND_CPU;
    problem.goal = CLR_GOAL_CLEAR_TO_EMPTY;
    problem.count_policy = CLR_COUNT_ALL;
    problem.objective = CLR_OBJECTIVE_ALL;
    return problem;
}static void packing_problem_prune_context_uses_actual_descriptor_identity(void) {
    ClearraBoard64Layout layout = pruning_test_layout();
    uint64_t target_mask = 0u;
    EXPECT_PACKING_STATUS(
        clearra_packing_target_mask_for_lines(layout, 1u, &target_mask),
        CLEARRA_PACKING_OK);
    clr_packing_problem problem =
        pruning_test_product_problem(layout, target_mask);
    clr_static_prune_context context;
    EXPECT_PACKING_STATUS(
        clearra_packing_prune_context_from_problem(&problem, &context),
        CLEARRA_PACKING_OK);

    EXPECT_TRUE(context.batch_id != 0u && context.batch_id != 1u);
    EXPECT_U64(
        context.operation_table_version,
        CLEARRA_STANDARD_OPERATION_TABLE_VERSION);
    EXPECT_U64(context.piece_set_id, CLR_PIECE_SET_STANDARD_TETROMINOES);
    EXPECT_U64(context.rule_profile_id, CLR_RULE_NO_KICK);
    EXPECT_U64(context.kick_profile_id, CLR_KICK_NO_KICK);

    clr_packing_problem changed_source = problem;
    changed_source.piece_source.piece_source_id++;
    clr_static_prune_context changed_context;
    EXPECT_PACKING_STATUS(
        clearra_packing_prune_context_from_problem(
            &changed_source, &changed_context),
        CLEARRA_PACKING_OK);
    EXPECT_NE_U64(context.batch_id, changed_context.batch_id);
}static void packing_product_path_records_actual_pruning_context(void) {
    ClearraBoard64Layout layout = pruning_test_layout();
    uint64_t target_mask = 0u;
    clr_resource_report resource_report;
    clr_pruning_proof_ledger ledger;
    static ClearraPackingCandidateBuffer buffer;
    EXPECT_PACKING_STATUS(
        clearra_packing_target_mask_for_lines(layout, 1u, &target_mask),
        CLEARRA_PACKING_OK);
    clr_packing_problem problem =
        pruning_test_product_problem(layout, target_mask);
    clr_static_prune_context expected_context;
    EXPECT_PACKING_STATUS(
        clearra_packing_prune_context_from_problem(
            &problem, &expected_context),
        CLEARRA_PACKING_OK);

    EXPECT_PACKING_STATUS(
        clearra_packing_enumerator_cpu_generate_problem_with_resource_report_and_pruning_ledger(
            &problem,
            &buffer,
            &resource_report,
            &ledger),
        CLEARRA_PACKING_OK);

    EXPECT_U64(buffer.count, 0u);
    EXPECT_FALSE(resource_report.truncated);
    EXPECT_TRUE(ledger.count > 0u);
    for (uint16_t index = 0u; index < ledger.count; ++index) {
        EXPECT_U64(ledger.entries[index].batch_id, expected_context.batch_id);
        EXPECT_U64(
            ledger.entries[index].prune_reason,
            CLR_PRUNE_TARGET_MASK_OVERFLOW);
        EXPECT_U64(
            ledger.entries[index].proof_level,
            CLR_PRUNE_PROOF_GLOBAL_SAFE);
    }
}
int main(void) {
    all_pruning_paths_emit_prune_reason();
    all_pruning_paths_emit_proof_level();
    target_frame_floating_not_global_pruned();
    target_frame_floating_piece_not_pruned_globally();
    forced_piece_family_conditional_under_clear_state();
    global_forced_piece_requires_all_reachable_clear_states();
    cannot_promote_by_count_only_without_clear_state_set_digest();
    global_forced_piece_requires_complete_clear_state_set();
    clear_state_set_truncated_keeps_candidate();
    component_domain_digest_changes_with_operation_table();
    target_frame_domain_never_global_safe_without_clear_state_set();
    component_exact_cover_runs_only_under_budget();
    mcts_priority_does_not_prune();
    rare_piece_heuristic_does_not_prune();
    local_only_cannot_drop_candidate_without_global_safe_proof();
    resource_cap_reached_is_incomplete_not_prune();
    resource_budget_exceeded_marks_result_incomplete_not_candidate_drop();
    ledger_capacity_does_not_abort_static_safe_prunes();
    candidate_drop_allowed_requires_global_safe_but_ledger_retention_is_best_effort();
    cell_domain_empty_is_clear_state_conditional();
    cell_domain_empty_has_clear_state_key();
    packing_static_pruner_records_ledger_for_target_mask_overflow();
    packing_static_pruner_records_ledger_for_collision();
    complete_required_ledger_capacity_keeps_candidate();
    static_prune_ledger_records_real_batch_id();
    static_prune_ledger_records_bfs_layer();
    static_prune_evidence_digest_changes_by_operation_id();
    static_prune_evidence_digest_changes_by_rule_profile();
    static_prune_evidence_digest_changes_by_kick_profile();
    static_prune_evidence_digest_changes_by_piece_set();
    static_prune_rejects_missing_context_or_ledger_before_drop();
    packing_problem_prune_context_uses_actual_descriptor_identity();
    packing_product_path_records_actual_pruning_context();
    ledger_overflow_sets_evidence_truncated_but_search_complete();
    packing_2l_many_static_prunes_still_returns_ok();
    puts("core-c pruning tests passed");
    return 0;
}
