#include "buildup_tests_support.h"

#include <string.h>
void buildup_test_set_piece_source_pattern_cache(
    clr_packing_problem *packing,
    const uint8_t *pieces,
    uint16_t count,
    uint8_t complete,
    uint16_t truncation_reason) {
    if (packing == 0) {
        return;
    }
    memset(packing->piece_source_pattern_pieces,
           CLR_PIECE_NONE,
           sizeof(packing->piece_source_pattern_pieces));
    uint16_t stored_count = count;
    if (stored_count > CLR_PIECE_SOURCE_PATTERN_READER_CAPACITY) {
        stored_count = CLR_PIECE_SOURCE_PATTERN_READER_CAPACITY;
    }
    for (uint16_t index = 0; pieces != 0 && index < stored_count; ++index) {
        packing->piece_source_pattern_pieces[index] = pieces[index];
    }
    packing->piece_source_pattern_len = stored_count;
    packing->piece_source_pattern_complete = complete;
    packing->piece_source_pattern_reserved = 0u;
    packing->piece_source_pattern_truncation_reason = truncation_reason;
    packing->piece_source_pattern_id = 0u;
}void buildup_test_set_board_descriptor(
    clr_board_descriptor *descriptor,
    uint16_t width,
    uint16_t visible_height,
    uint16_t search_height,
    uint64_t initial_mask) {
    if (clr_board_descriptor_init(
            width, visible_height, search_height, initial_mask, 0, descriptor) !=
        CLR_BOARD_OK) {
        fprintf(stderr, "failed to initialize board descriptor\n");
        exit(1);
    }
}ClearraCacheIdentity buildup_test_full_cache_identity(void) {
    ClearraCacheIdentity identity = clearra_cache_identity_zero();
    identity.board = UINT64_C(0x1234);
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
}uint64_t buildup_test_low_mask_for_cells(uint32_t cell_count) {
    if (cell_count >= 64u) {
        return UINT64_MAX;
    }
    return (UINT64_C(1) << cell_count) - UINT64_C(1);
}clr_packing_problem buildup_test_valid_packing_problem(void) {
    clr_packing_problem problem = clr_packing_problem_zero();
    problem.problem_kind = CLR_PROBLEM_SCENARIO_PC;
    problem.max_pieces = 5;
    buildup_test_set_board_descriptor(&problem.board, 10, 2, 4, UINT64_C(0x30));
    problem.goal_region_mask = buildup_test_low_mask_for_cells(20);
    problem.required_fill_mask = problem.goal_region_mask & ~problem.board.initial_mask;
    problem.exact_pieces = 0;
    problem.piece_window.max_pieces = 5;
    problem.piece_window.exact_pieces = 0;
    problem.piece_window.has_exact_pieces = 0;
    uint8_t pieces[] = {
        CLR_PIECE_O,
        CLR_PIECE_I,
    };
    problem.piece_multiset_window =
        clearra_piece_multiset_window_from_pieces(pieces, 2);
    problem.piece_source = clearra_piece_source_descriptor_fixed_queue(
        1u,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE,
        2,
        CLR_PIECE_SET_STANDARD_TETROMINOES);
    buildup_test_set_piece_source_pattern_cache(
        &problem,
        pieces,
        2,
        1u,
        CLR_SUPPLY_TRUNCATION_NONE);
    problem.rule.piece_set_profile_id = CLR_PIECE_SET_STANDARD_TETROMINOES;
    problem.rule.bag_profile_id = CLR_BAG_STANDARD_7_BAG;
    problem.rule.rule_profile_id = CLR_RULE_SRS_PLUS;
    problem.rule.kick_profile_id = CLR_KICK_SRS_PLUS_180;
    problem.rule.spawn_profile_id = CLR_SPAWN_STANDARD_10;
    problem.backend.requested_backend = CLR_BACKEND_CPU;
    problem.backend.workers = 1;
    problem.backend.deterministic = 1;
    problem.backend.fallback_policy = CLR_BACKEND_FALLBACK_DENY;
    problem.goal = CLR_GOAL_CLEAR_TO_EMPTY;
    problem.count_policy = CLR_COUNT_ALL;
    problem.objective = CLR_OBJECTIVE_ALL;
    return problem;
}ClearraPackingCandidateView buildup_test_two_operation_candidate(void) {
    ClearraPackingCandidateView candidate;
    clearra_packing_candidate_view_clear(&candidate);
    candidate.placed_count = 2;
    candidate.cleared_lines = 0;
    candidate.pieces[0] = CLR_PIECE_O;
    candidate.rotations[0] = 0;
    candidate.xs[0] = 0;
    candidate.ys[0] = 0;
    candidate.operation_ids[0] = 4;
    candidate.operation_masks[0] = UINT64_C(0x0000000000000c03);
    candidate.pieces[1] = CLR_PIECE_I;
    candidate.rotations[1] = 0;
    candidate.xs[1] = 2;
    candidate.ys[1] = 0;
    candidate.operation_ids[1] = 0;
    candidate.operation_masks[1] = UINT64_C(0x000000000000003c);
    return candidate;
}ClearraBoard64Layout buildup_test_standard_10x2_layout(void) {
    ClearraBoard64Layout layout;
    if (clearra_board64_make_layout(10, 2, &layout) != CLEARRA_BOARD64_OK) {
        fprintf(stderr, "failed to make 10x2 layout\n");
        exit(1);
    }
    return layout;
}ClearraBoard64Layout buildup_test_standard_10x4_layout(void) {
    ClearraBoard64Layout layout;
    if (clearra_board64_make_layout(10, 4, &layout) != CLEARRA_BOARD64_OK) {
        fprintf(stderr, "failed to make 10x4 layout\n");
        exit(1);
    }
    return layout;
}uint64_t buildup_test_cell_mask(ClearraBoard64Layout layout, uint8_t x, uint8_t y) {
    uint8_t index = 0;
    if (clearra_board64_cell_index(layout, x, y, &index) != CLEARRA_BOARD64_OK) {
        fprintf(stderr, "failed to make cell mask\n");
        exit(1);
    }
    return UINT64_C(1) << index;
}uint64_t buildup_test_o_mask_at(ClearraBoard64Layout layout, uint8_t x, uint8_t y) {
    return buildup_test_cell_mask(layout, x, y) | buildup_test_cell_mask(layout, (uint8_t)(x + 1u), y) |
           buildup_test_cell_mask(layout, x, (uint8_t)(y + 1u)) |
           buildup_test_cell_mask(layout, (uint8_t)(x + 1u), (uint8_t)(y + 1u));
}uint64_t buildup_test_t_spawn_mask_at(ClearraBoard64Layout layout, uint8_t x, uint8_t y) {
    return buildup_test_cell_mask(layout, x, y) | buildup_test_cell_mask(layout, (uint8_t)(x + 1u), y) |
           buildup_test_cell_mask(layout, (uint8_t)(x + 2u), y) |
           buildup_test_cell_mask(layout, (uint8_t)(x + 1u), (uint8_t)(y + 1u));
}clr_packing_problem buildup_test_buildup_packing_problem(
    uint16_t height,
    uint16_t exact_pieces,
    uint8_t hold_enabled) {
    clr_packing_problem packing = clr_packing_problem_zero();
    packing.problem_kind = CLR_PROBLEM_SCENARIO_PC;
    packing.max_pieces = exact_pieces;
    buildup_test_set_board_descriptor(&packing.board, 10, height, height, 0);
    packing.goal_region_mask =
        buildup_test_low_mask_for_cells((uint32_t)packing.board.width * packing.board.visible_height);
    packing.required_fill_mask = packing.goal_region_mask;
    packing.exact_pieces = exact_pieces;
    packing.piece_window.max_pieces = exact_pieces;
    packing.piece_window.exact_pieces = exact_pieces;
    packing.piece_window.has_exact_pieces = 1;
    uint8_t pieces[CLR_PIECE_MULTISET_WINDOW_CAPACITY] = {0};
    for (uint16_t index = 0u;
         index < exact_pieces && index < CLR_PIECE_MULTISET_WINDOW_CAPACITY;
         ++index) {
        pieces[index] = CLR_PIECE_O;
    }
    packing.piece_multiset_window =
        clearra_piece_multiset_window_from_pieces(pieces, exact_pieces);
    packing.piece_source = clearra_piece_source_descriptor_fixed_queue(
        1u,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE,
        exact_pieces,
        CLR_PIECE_SET_STANDARD_TETROMINOES);
    buildup_test_set_piece_source_pattern_cache(
        &packing,
        pieces,
        exact_pieces,
        1u,
        CLR_SUPPLY_TRUNCATION_NONE);
    packing.flags = hold_enabled ? CLR_BUILDUP_FLAG_HOLD_ENABLED : 0u;
    packing.rule.piece_set_profile_id = CLR_PIECE_SET_STANDARD_TETROMINOES;
    packing.rule.bag_profile_id = CLR_BAG_STANDARD_7_BAG;
    packing.rule.rule_profile_id = CLR_RULE_NO_KICK;
    packing.rule.kick_profile_id = CLR_KICK_NO_KICK;
    packing.rule.spawn_profile_id = CLR_SPAWN_STANDARD_10;
    packing.backend.requested_backend = CLR_BACKEND_CPU;
    packing.backend.workers = 1;
    packing.backend.deterministic = 1;
    packing.backend.fallback_policy = CLR_BACKEND_FALLBACK_DENY;
    packing.goal = CLR_GOAL_CLEAR_TO_EMPTY;
    packing.count_policy = CLR_COUNT_ALL;
    packing.objective = CLR_OBJECTIVE_ALL;
    return packing;
}void buildup_test_set_packing_pieces(
    clr_packing_problem *packing,
    const uint8_t *pieces,
    uint16_t count,
    uint32_t source_kind,
    uint32_t provenance_id) {
    if (packing == 0) {
        return;
    }
    uint16_t multiset_count = count;
    if (packing->piece_window.max_pieces != 0u &&
        multiset_count > packing->piece_window.max_pieces) {
        multiset_count = packing->piece_window.max_pieces;
    }
    packing->piece_multiset_window =
        clearra_piece_multiset_window_from_pieces(pieces, multiset_count);
    uint8_t complete = 1u;
    uint16_t truncation_reason = CLR_SUPPLY_TRUNCATION_NONE;
    if (source_kind == CLR_PIECE_SOURCE_BAG_UNIVERSE) {
        packing->piece_source = clearra_piece_source_descriptor_bag_universe(
            1u,
            provenance_id,
            CLR_PIECE_SET_STANDARD_TETROMINOES);
    } else if (source_kind == CLR_PIECE_SOURCE_OBSERVED_WINDOW) {
        packing->piece_source = clearra_piece_source_descriptor_observed_window(
            1u,
            provenance_id,
            CLR_PIECE_SET_STANDARD_TETROMINOES,
            true,
            CLR_SUPPLY_TRUNCATION_NONE);
    } else {
        packing->piece_source = clearra_piece_source_descriptor_fixed_queue(
            1u,
            provenance_id,
            count,
            CLR_PIECE_SET_STANDARD_TETROMINOES);
    }
    buildup_test_set_piece_source_pattern_cache(
        packing,
        pieces,
        count,
        complete,
        truncation_reason);
}void buildup_test_configure_initial_hold(
    clr_buildup_problem *problem,
    uint8_t hold_enabled,
    uint8_t hold_empty,
    uint8_t hold_piece) {
    if (problem == 0) {
        return;
    }
    if (hold_enabled != 0u) {
        problem->buildup_flags |= CLR_BUILDUP_FLAG_HOLD_ENABLED;
    } else {
        problem->buildup_flags &= ~CLR_BUILDUP_FLAG_HOLD_ENABLED;
    }
    problem->initial_hold_automaton.hold_empty = hold_empty ? 1u : 0u;
    problem->initial_hold_automaton.hold_piece =
        hold_empty ? CLR_PIECE_NONE : hold_piece;
}clr_rule_profile_descriptor buildup_test_rule_descriptor(uint32_t rule, uint32_t kick) {
    clr_rule_profile_descriptor descriptor = {0};
    descriptor.piece_set_profile_id = CLR_PIECE_SET_STANDARD_TETROMINOES;
    descriptor.bag_profile_id = CLR_BAG_STANDARD_7_BAG;
    descriptor.rule_profile_id = rule;
    descriptor.kick_profile_id = kick;
    descriptor.spawn_profile_id = CLR_SPAWN_STANDARD_10;
    return descriptor;
}clr_rule_profile_descriptor buildup_test_imported_verified_kick_descriptor(void) {
    clr_rule_profile_descriptor descriptor =
        buildup_test_rule_descriptor(CLR_RULE_SRS_X, CLR_KICK_IMPORTED);
    descriptor.has_verified_kick_profile = 1;
    descriptor.verified_supports_180 = 1;
    descriptor.verified_transition_count = 2;
    descriptor.verified_transitions[0].piece = CLR_PIECE_T;
    descriptor.verified_transitions[0].from_rotation = CLEARRA_RULE_ROTATION_SPAWN;
    descriptor.verified_transitions[0].to_rotation = CLEARRA_RULE_ROTATION_RIGHT;
    descriptor.verified_transitions[0].sequence.count = 1;
    descriptor.verified_transitions[0].sequence.offsets[0].dx = 0;
    descriptor.verified_transitions[0].sequence.offsets[0].dy = 0;
    descriptor.verified_transitions[1].piece = CLR_PIECE_T;
    descriptor.verified_transitions[1].from_rotation = CLEARRA_RULE_ROTATION_SPAWN;
    descriptor.verified_transitions[1].to_rotation = CLEARRA_RULE_ROTATION_REVERSE;
    descriptor.verified_transitions[1].sequence.count = 1;
    descriptor.verified_transitions[1].sequence.offsets[0].dx = 0;
    descriptor.verified_transitions[1].sequence.offsets[0].dy = 0;
    return descriptor;
}ClearraPackingCandidateView buildup_test_o_candidate_for_columns(
    ClearraBoard64Layout layout,
    const uint8_t *columns,
    uint8_t count) {
    ClearraPackingCandidateView candidate;
    clearra_packing_candidate_view_clear(&candidate);
    candidate.placed_count = count;
    for (uint8_t index = 0; index < count; index++) {
        candidate.pieces[index] = CLR_PIECE_O;
        candidate.rotations[index] = 0;
        candidate.xs[index] = (int8_t)columns[index];
        candidate.ys[index] = 0;
        candidate.operation_ids[index] = index;
        candidate.operation_masks[index] = buildup_test_o_mask_at(layout, columns[index], 0);
    }
    return candidate;
}ClearraPackingCandidateView buildup_test_representative_order_hint_is_not_solution_order_candidate(
    ClearraBoard64Layout layout) {
    ClearraPackingCandidateView candidate;
    clearra_packing_candidate_view_clear(&candidate);
    candidate.placed_count = 2;

    candidate.pieces[0] = CLR_PIECE_O;
    candidate.rotations[0] = CLEARRA_ROTATION_SPAWN;
    candidate.xs[0] = 3;
    candidate.ys[0] = 0;
    candidate.operation_masks[0] = buildup_test_o_mask_at(layout, 3, 0);
    if (clearra_operation_id(CLR_PIECE_O, CLEARRA_ROTATION_SPAWN,
                             &candidate.operation_ids[0]) != CLEARRA_OPERATION_OK) {
        fprintf(stderr, "failed to create O operation id\n");
        exit(1);
    }

    candidate.pieces[1] = CLR_PIECE_T;
    candidate.rotations[1] = CLEARRA_ROTATION_REVERSE;
    candidate.xs[1] = 0;
    candidate.ys[1] = 0;
    if (clearra_candidate_mask_for_piece(
            layout, CLR_PIECE_T, CLEARRA_ROTATION_REVERSE, 0, 0,
            &candidate.operation_masks[1]) != CLEARRA_CANDIDATE_OK) {
        fprintf(stderr, "failed to create reverse T mask\n");
        exit(1);
    }
    if (clearra_operation_id(CLR_PIECE_T, CLEARRA_ROTATION_REVERSE,
                             &candidate.operation_ids[1]) != CLEARRA_OPERATION_OK) {
        fprintf(stderr, "failed to create T operation id\n");
        exit(1);
    }

    return candidate;
}clr_buildup_problem buildup_test_build_problem_from_candidate(
    clr_packing_problem packing,
    ClearraPackingCandidateView candidate) {
    clr_buildup_problem problem;
    EXPECT_U64(clearra_buildup_problem_from_packing_candidate(
                   &packing, &candidate, 17, &problem),
               CLEARRA_PACKING_OK);
    return problem;
}void buildup_test_assert_buildup_reachability_bridge_uses_rule_kick_table(
    clr_rule_profile_descriptor rule,
    uint8_t piece,
    uint8_t rotation,
    uint32_t expected_kick_profile,
    bool expected_180_support) {
    ClearraBoard64Layout layout = buildup_test_standard_10x4_layout();

    clr_buildup_operation operation = {0};
    operation.piece = piece;
    operation.rotation = rotation;
    operation.x = 0;
    operation.y = 0;
    EXPECT_U64(clearra_operation_id(piece, rotation, &operation.operation_id),
               CLEARRA_OPERATION_OK);
    EXPECT_U64(clearra_candidate_mask_for_piece(
                   layout, piece, rotation, operation.x, operation.y, &operation.mask),
               CLEARRA_CANDIDATE_OK);

    clr_buildup_problem problem = {0};
    problem.rule = rule;

    ClearraReachabilityKickTable kick_table;
    EXPECT_U64(clearra_reachability_kick_table_from_rule(
                   &problem.rule, operation.piece, &kick_table),
               CLEARRA_REACHABILITY_OK);
    EXPECT_TRUE(kick_table.compact_table != 0);
    EXPECT_U64(kick_table.compact_table->kick_profile_id, expected_kick_profile);
    EXPECT_U64(kick_table.compact_table->supports_180, expected_180_support ? 1 : 0);
    EXPECT_TRUE(kick_table.compact_table->transition_count > 0);

    EXPECT_BUILDUP_STATUS(clearra_buildup_reachability_bridge_accepts(
                              &problem, layout, clearra_board64_empty(), &operation, 0, 0),
                          CLR_BUILDUP_OK);
}
