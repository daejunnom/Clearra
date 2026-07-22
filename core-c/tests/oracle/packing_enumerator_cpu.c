#include "packing/packing_problem.h"
#include "clr_search_profile.h"
static ClearraPackingStatus problem_piece_count_range(
    const clr_packing_problem *problem,
    uint8_t *out_min_piece_count,
    uint8_t *out_max_piece_count) {
    if (problem == 0 || out_min_piece_count == 0 || out_max_piece_count == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    uint16_t min_piece_count = 1u;
    uint16_t max_piece_count = problem->piece_window.max_pieces;
    if (problem->piece_window.has_exact_pieces) {
        min_piece_count = problem->piece_window.exact_pieces;
        max_piece_count = problem->piece_window.exact_pieces;
    }
    if (min_piece_count == 0u || max_piece_count < min_piece_count ||
        max_piece_count > CLEARRA_PACKING_MAX_PIECES) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    if (max_piece_count > problem->piece_multiset_window.total_count) {
        if (!clearra_piece_source_descriptor_is_complete(&problem->piece_source)) {
            return CLEARRA_PACKING_INVALID_ARGUMENT;
        }
        max_piece_count = problem->piece_multiset_window.total_count;
    }
    *out_min_piece_count = (uint8_t)min_piece_count;
    *out_max_piece_count = (uint8_t)max_piece_count;
    return CLEARRA_PACKING_OK;
}static bool problem_piece_multiset_is_too_short_for_minimum_piece_count(
    const clr_packing_problem *problem) {
    uint16_t min_piece_count = 1u;
    if (problem->piece_window.has_exact_pieces) {
        min_piece_count = problem->piece_window.exact_pieces;
    }
    return min_piece_count == 0u ||
           problem->piece_multiset_window.total_count < min_piece_count;
}static uint32_t count_bits64(uint64_t value) {
    uint32_t count = 0u;
    while (value != 0u) {
        count += (uint32_t)(value & UINT64_C(1));
        value >>= 1u;
    }
    return count;
}static uint32_t problem_available_piece_area(
    const clr_packing_problem *problem) {
    uint32_t total_area = 0u;
    if (problem == 0) {
        return 0u;
    }
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        total_area +=
            (uint32_t)problem->piece_multiset_window.counts[piece] *
            (uint32_t)clearra_piece_area(piece);
    }
    return total_area;
}static bool problem_area_cannot_fill_required_region(
    const clr_packing_problem *problem) {
    if (problem == 0) {
        return true;
    }
    uint64_t missing_required_mask =
        problem->required_fill_mask & ~problem->board.initial_mask;
    return problem_available_piece_area(problem) < count_bits64(missing_required_mask);
}ClearraPackingStatus clearra_packing_enumerator_cpu_generate_problem(
    const clr_packing_problem *problem,
    ClearraPackingCandidateBuffer *out_buffer) {
    clr_resource_report report;
    return clearra_packing_enumerator_cpu_generate_problem_with_resource_report(
        problem, out_buffer, &report);
}ClearraPackingStatus clearra_packing_enumerator_cpu_generate_problem_with_resource_report(
    const clr_packing_problem *problem,
    ClearraPackingCandidateBuffer *out_buffer,
    clr_resource_report *out_resource_report) {
    clr_pruning_proof_ledger pruning_ledger;
    return clearra_packing_enumerator_cpu_generate_problem_with_resource_report_and_pruning_ledger(
        problem, out_buffer, out_resource_report, &pruning_ledger);
}ClearraPackingStatus
clearra_packing_enumerator_cpu_generate_problem_with_resource_report_and_pruning_ledger(
    const clr_packing_problem *problem,
    ClearraPackingCandidateBuffer *out_buffer,
    clr_resource_report *out_resource_report,
    clr_pruning_proof_ledger *out_pruning_ledger) {
    return clearra_packing_enumerator_cpu_generate_problem_with_resource_report_pruning_policy_and_ledger(
        problem,
        out_buffer,
        out_resource_report,
        CLR_PRUNING_EVIDENCE_BEST_EFFORT,
        out_pruning_ledger);
}static ClearraPackingStatus generate_problem_with_candidate_output(
    const clr_packing_problem *problem,
    ClearraPackingCandidateBuffer *out_buffer,
    const ClearraPackingCandidateSink *sink,
    uint16_t partition_index,
    uint16_t partition_count,
    uint8_t partition_depth,
    clr_resource_report *out_resource_report,
    clr_pruning_evidence_policy evidence_policy,
    clr_pruning_proof_ledger *out_pruning_ledger) {
    clr_search_profile_span total_span =
        clr_search_profile_begin(CLR_PROFILE_PACKING_TOTAL);
    clr_search_profile_span validation_span =
        clr_search_profile_begin(CLR_PROFILE_PACKING_VALIDATE_AND_LOWER);
    bool has_buffer = out_buffer != 0;
    bool has_sink = sink != 0 && sink->consume != 0;
    if (problem == 0 || has_buffer == has_sink || partition_count == 0u ||
        partition_index >= partition_count || partition_depth == 0u ||
        out_resource_report == 0 ||
        out_pruning_ledger == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    clr_resource_report_clear(out_resource_report);
    if (clr_pruning_proof_ledger_init_with_policy(
            out_pruning_ledger,
            evidence_policy) != CLR_PRUNING_OK) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    if (!clr_packing_problem_is_valid(problem)) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    if (problem_piece_multiset_is_too_short_for_minimum_piece_count(problem)) {
        if (out_buffer != 0) {
            clearra_packing_candidate_buffer_clear(out_buffer);
        }
        if (clearra_piece_source_descriptor_is_complete(&problem->piece_source)) {
            return CLEARRA_PACKING_OK;
        }
        clr_resource_report_mark_truncated(
            out_resource_report,
            CLR_RESOURCE_TRUNCATION_OBSERVED_UNIVERSE_TRUNCATED);
        return CLEARRA_PACKING_CAPACITY_EXCEEDED;
    }
    if (problem_area_cannot_fill_required_region(problem)) {
        if (out_buffer != 0) {
            clearra_packing_candidate_buffer_clear(out_buffer);
        }
        return CLEARRA_PACKING_OK;
    }

    uint8_t min_piece_count = 0;
    uint8_t max_piece_count = 0;
    ClearraPackingStatus count_status =
        problem_piece_count_range(problem, &min_piece_count, &max_piece_count);
    if (count_status != CLEARRA_PACKING_OK) {
        return count_status;
    }
    if (partition_depth > max_piece_count) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }

    (void)clr_search_profile_end(validation_span, 1u);
    clr_resource_report catalog_report;
    ClearraGeometryCatalog *catalog = 0;
    ClearraPackingStatus status = clearra_geometry_catalog_compile(
        problem,
        &catalog_report,
        evidence_policy,
        out_pruning_ledger,
        &catalog);
    if (status == CLEARRA_PACKING_OK) {
        status = out_buffer != 0
            ? clearra_geometry_exact_cover_search_to_buffer(
                  catalog, problem, out_buffer, out_resource_report)
            : clearra_geometry_exact_cover_search_to_sink(
                  catalog,
                  problem,
                  partition_index,
                  partition_count,
                  partition_depth,
                  sink,
                  out_resource_report);
    } else {
        *out_resource_report = catalog_report;
    }
    if (catalog != 0) {
        clr_resource_report_observe_hash_buckets(
            out_resource_report, catalog_report.peak_hash_buckets);
        clr_resource_report_observe_cpu_bytes(
            out_resource_report, catalog_report.peak_cpu_bytes);
    }
    clearra_geometry_catalog_release(&catalog);
    (void)clr_search_profile_end(total_span, 1u);
    return status;
}

ClearraPackingStatus
clearra_packing_enumerator_cpu_generate_problem_with_resource_report_pruning_policy_and_ledger(
    const clr_packing_problem *problem,
    ClearraPackingCandidateBuffer *out_buffer,
    clr_resource_report *out_resource_report,
    clr_pruning_evidence_policy evidence_policy,
    clr_pruning_proof_ledger *out_pruning_ledger) {
    return generate_problem_with_candidate_output(
        problem,
        out_buffer,
        0,
        0u,
        1u,
        1u,
        out_resource_report,
        evidence_policy,
        out_pruning_ledger);
}

ClearraPackingStatus
clearra_packing_enumerator_cpu_generate_problem_to_sink_with_resource_report_and_pruning_ledger(
    const clr_packing_problem *problem,
    const ClearraPackingCandidateSink *sink,
    clr_resource_report *out_resource_report,
    clr_pruning_proof_ledger *out_pruning_ledger) {
    return generate_problem_with_candidate_output(
        problem,
        0,
        sink,
        0u,
        1u,
        1u,
        out_resource_report,
        CLR_PRUNING_EVIDENCE_BEST_EFFORT,
        out_pruning_ledger);
}

ClearraPackingStatus
clearra_packing_enumerator_cpu_generate_problem_partition_to_sink_with_resource_report_and_pruning_ledger(
    const clr_packing_problem *problem,
    uint16_t root_partition_index,
    uint16_t root_partition_count,
    const ClearraPackingCandidateSink *sink,
    clr_resource_report *out_resource_report,
    clr_pruning_proof_ledger *out_pruning_ledger) {
    return clearra_packing_enumerator_cpu_generate_problem_prefix_partition_to_sink_with_resource_report_and_pruning_ledger(
        problem,
        root_partition_index,
        root_partition_count,
        1u,
        sink,
        out_resource_report,
        out_pruning_ledger);
}

ClearraPackingStatus
clearra_packing_enumerator_cpu_generate_problem_prefix_partition_to_sink_with_resource_report_and_pruning_ledger(
    const clr_packing_problem *problem,
    uint16_t partition_index,
    uint16_t partition_count,
    uint8_t partition_depth,
    const ClearraPackingCandidateSink *sink,
    clr_resource_report *out_resource_report,
    clr_pruning_proof_ledger *out_pruning_ledger) {
    return generate_problem_with_candidate_output(
        problem,
        0,
        sink,
        partition_index,
        partition_count,
        partition_depth,
        out_resource_report,
        CLR_PRUNING_EVIDENCE_BEST_EFFORT,
        out_pruning_ledger);
}

ClearraPackingStatus clearra_packing_enumerator_cpu_generate(
    ClearraBoard64Layout layout,
    uint64_t initial_board,
    uint8_t target_lines,
    const uint8_t *pieces,
    uint8_t piece_count,
    ClearraPackingCandidateBuffer *out_buffer) {
    if (pieces == 0 || out_buffer == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }

    uint64_t target_mask = 0;
    ClearraPackingStatus target_status =
        clearra_packing_target_mask_for_lines(layout, target_lines, &target_mask);
    if (target_status != CLEARRA_PACKING_OK) {
        return target_status;
    }

    clr_packing_problem problem = clr_packing_problem_zero();
    problem.problem_kind = CLR_PROBLEM_OPENING_PC;
    problem.max_pieces = piece_count;
    problem.board.width = layout.width;
    problem.board.visible_height = layout.height;
    problem.board.search_height = layout.height;
    problem.board.initial_mask = initial_board;
    problem.board.initial_mask_hi = 0;
    problem.board.backend_kind = CLR_BOARD_BACKEND_BOARD64;
    problem.board.cell_count = layout.cell_count;
    problem.goal_region_mask = target_mask;
    problem.required_fill_mask = target_mask & ~initial_board;
    problem.forbidden_mask = 0;
    problem.exact_pieces = piece_count;
    problem.piece_window =
        clearra_piece_window_descriptor(piece_count, piece_count, true);
    problem.piece_multiset_window =
        clearra_piece_multiset_window_from_pieces(pieces, piece_count);
    problem.piece_source.piece_source_id = 1u;
    problem.piece_source.source_kind = CLR_PIECE_SOURCE_FIXED_QUEUE;
    problem.piece_source.provenance_id = CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE;
    problem.piece_source.fixed_sequence_len = piece_count;
    problem.piece_source.piece_set_profile_id =
        CLR_PIECE_SET_STANDARD_TETROMINOES;
    problem.piece_source.complete = 1u;
    problem.rule.piece_set_profile_id = CLR_PIECE_SET_STANDARD_TETROMINOES;
    problem.rule.bag_profile_id = CLR_BAG_STANDARD_7_BAG;
    problem.rule.rule_profile_id = CLR_RULE_SRS_PLUS;
    problem.rule.kick_profile_id = CLR_KICK_SRS_PLUS_180;
    problem.rule.spawn_profile_id = CLR_SPAWN_STANDARD_10;
    problem.goal = CLR_GOAL_CLEAR_TO_EMPTY;
    problem.count_policy = CLR_COUNT_ALL;
    problem.objective = CLR_OBJECTIVE_ALL;
    return clearra_packing_enumerator_cpu_generate_problem(&problem, out_buffer);
}
