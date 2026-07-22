#include "../packing/packing_problem.h"

#include <string.h>

static ClearraPackingStatus narrow_packing_multiset_to_candidate(
    clr_packing_problem *packing,
    const ClearraPackingCandidateView *candidate) {
    packing->piece_multiset_window = clearra_piece_multiset_window_empty();
    packing->piece_multiset_family = clearra_piece_multiset_family_empty();
    packing->piece_multiset_window.total_count = candidate->placed_count;
    packing->piece_multiset_window.exact_count = candidate->placed_count;

    for (uint8_t index = 0u; index < candidate->placed_count; ++index) {
        uint8_t piece = candidate->pieces[index];
        if (piece < CLR_PIECE_I || piece > CLR_PIECE_L) {
            return CLEARRA_PACKING_INVALID_PIECE;
        }
        packing->piece_multiset_window.counts[piece]++;
    }
    return CLEARRA_PACKING_OK;
}

static void copy_packing_fields(
    const clr_packing_problem *packing,
    clr_buildup_problem *buildup) {
    buildup->packing = *packing;
    buildup->initial_board = packing->board;
    buildup->piece_source = packing->piece_source;
    memcpy(buildup->piece_source_pattern_pieces,
           packing->piece_source_pattern_pieces,
           sizeof(buildup->piece_source_pattern_pieces));
    buildup->piece_source_pattern_len = packing->piece_source_pattern_len;
    buildup->piece_source_pattern_complete =
        packing->piece_source_pattern_complete;
    buildup->piece_source_pattern_reserved = 0u;
    buildup->piece_source_pattern_truncation_reason =
        packing->piece_source_pattern_truncation_reason;
    buildup->piece_source_pattern_id = packing->piece_source_pattern_id;
    buildup->initial_hold_automaton.piece_source_id =
        packing->piece_source.piece_source_id;
    buildup->initial_hold_automaton.cursor = 0u;
    buildup->initial_hold_automaton.bag_epoch = 0u;
    buildup->initial_hold_automaton.bag_remainder_key = 0u;
    buildup->initial_hold_automaton.provenance_id =
        packing->piece_source.provenance_id;
    buildup->initial_hold_automaton.hold_piece = CLR_PIECE_NONE;
    buildup->initial_hold_automaton.hold_empty = 1u;
    buildup->rule = packing->rule;
    buildup->line_clear_policy = CLR_LINE_CLEAR_POLICY_STANDARD;
    buildup->piece_window = packing->piece_window;
    buildup->goal = packing->goal;
    buildup->buildup_flags = packing->flags & CLR_BUILDUP_FLAG_HOLD_ENABLED;
    buildup->source_execution_mode = CLR_BUILDUP_SOURCE_CONCRETE_PATTERN;
}clr_buildup_problem clr_buildup_problem_from_packing(clr_packing_problem problem) {
    clr_buildup_problem buildup = {0};
    copy_packing_fields(&problem, &buildup);
    return buildup;
}bool clr_buildup_problem_is_valid(const clr_buildup_problem *problem) {
    if (problem == 0 || !clr_packing_problem_is_valid(&problem->packing)) {
        return false;
    }
    if (!clr_board_descriptor_is_valid(&problem->initial_board)) {
        return false;
    }
    if (problem->operation_set.operation_count > CLR_BUILDUP_MAX_OPERATIONS ||
        problem->operation_set.operation_count > problem->piece_window.max_pieces) {
        return false;
    }
    uint16_t valid_operation_bits =
        problem->operation_set.operation_count == 0u
            ? 0u
            : (uint16_t)((UINT16_C(1)
                          << problem->operation_set.operation_count) -
                         UINT16_C(1));
    if ((problem->operation_set.geometry_variant_domains &
         (uint16_t)~valid_operation_bits) != 0u) {
        return false;
    }
    if (!clearra_piece_source_descriptor_valid(&problem->piece_source) ||
        problem->piece_source.piece_source_id !=
            problem->packing.piece_source.piece_source_id) {
        return false;
    }
    if (problem->initial_hold_automaton.piece_source_id !=
        problem->piece_source.piece_source_id) {
        return false;
    }
    if (problem->rule.rule_profile_id == 0 || problem->rule.kick_profile_id == 0) {
        return false;
    }
    if (problem->line_clear_policy != CLR_LINE_CLEAR_POLICY_STANDARD) {
        return false;
    }
    if (problem->source_execution_mode != CLR_BUILDUP_SOURCE_CONCRETE_PATTERN &&
        problem->source_execution_mode != CLR_BUILDUP_SOURCE_STANDARD_BAG_AUTOMATON) {
        return false;
    }
    return problem->goal == CLR_GOAL_CLEAR_TO_EMPTY;
}ClearraPackingStatus clearra_buildup_problem_from_packing_candidate(
    const clr_packing_problem *packing,
    const ClearraPackingCandidateView *candidate,
    uint32_t coverage_pattern_id,
    clr_buildup_problem *out_problem) {
    if (packing == 0 || candidate == 0 || out_problem == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    if (!clr_packing_problem_is_valid(packing)) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    if (candidate->placed_count > CLR_BUILDUP_MAX_OPERATIONS ||
        candidate->placed_count > packing->piece_window.max_pieces) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }

    *out_problem = clr_buildup_problem_from_packing(*packing);
    return clearra_buildup_problem_apply_packing_candidate(
        out_problem, candidate, coverage_pattern_id);
}

ClearraPackingStatus clearra_buildup_problem_apply_packing_candidate(
    clr_buildup_problem *problem,
    const ClearraPackingCandidateView *candidate,
    uint32_t coverage_pattern_id) {
    if (problem == 0 || candidate == 0 ||
        candidate->placed_count > CLR_BUILDUP_MAX_OPERATIONS ||
        candidate->placed_count > problem->piece_window.max_pieces) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    ClearraPackingStatus multiset_status =
        narrow_packing_multiset_to_candidate(&problem->packing, candidate);
    if (multiset_status != CLEARRA_PACKING_OK) {
        return multiset_status;
    }
    problem->candidate_id = candidate->candidate_id;
    problem->canonical_operation_set_id =
        candidate->canonical_operation_set_id;
    problem->coverage_pattern_id = coverage_pattern_id;
    problem->piece_source_pattern_id = coverage_pattern_id;
    problem->packing.piece_source_pattern_id = coverage_pattern_id;
    problem->operation_set.operation_count = candidate->placed_count;
    problem->operation_set.geometry_variant_domains =
        candidate->geometry_variant_domains;
    for (uint8_t index = 0; index < candidate->placed_count; index++) {
        problem->operation_set.representative_order_hint[index] = index;
        problem->operation_set.operations[index].piece = candidate->pieces[index];
        problem->operation_set.operations[index].rotation =
            candidate->rotations[index];
        problem->operation_set.operations[index].x = candidate->xs[index];
        problem->operation_set.operations[index].y = candidate->ys[index];
        problem->operation_set.operations[index].operation_id =
            candidate->operation_ids[index];
        problem->operation_set.operations[index].required_deleted_row_mask =
            candidate->operation_deleted_row_masks[index];
        problem->operation_set.operations[index].mask =
            candidate->operation_masks[index];
    }

    return clr_buildup_problem_is_valid(problem)
        ? CLEARRA_PACKING_OK
        : CLEARRA_PACKING_INVALID_ARGUMENT;
}
