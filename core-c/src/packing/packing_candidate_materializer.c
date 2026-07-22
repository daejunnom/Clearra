#include "packing_candidate_materializer.h"
#include "geometry_catalog_internal.h"

#include "../cache/cache_identity.h"
#include "clr_execution_control.h"

#include <limits.h>

static int compare_operation_at(
    const ClearraPackingCandidateView *candidate,
    uint8_t left,
    uint8_t right) {
#define COMPARE_FIELD(field)                                              \
    if (candidate->field[left] != candidate->field[right]) {             \
        return candidate->field[left] < candidate->field[right] ? -1 : 1; \
    }
    COMPARE_FIELD(pieces)
    COMPARE_FIELD(rotations)
    COMPARE_FIELD(xs)
    COMPARE_FIELD(ys)
    COMPARE_FIELD(operation_ids)
    COMPARE_FIELD(operation_deleted_row_masks)
    COMPARE_FIELD(operation_masks)
#undef COMPARE_FIELD
    return 0;
}

static void swap_operations(
    ClearraPackingCandidateView *candidate,
    uint8_t left,
    uint8_t right) {
#define SWAP_FIELD(type, field)                              \
    do {                                                     \
        type value = candidate->field[left];                 \
        candidate->field[left] = candidate->field[right];    \
        candidate->field[right] = value;                     \
    } while (0)
    SWAP_FIELD(uint8_t, pieces);
    SWAP_FIELD(uint8_t, rotations);
    SWAP_FIELD(int8_t, xs);
    SWAP_FIELD(int8_t, ys);
    SWAP_FIELD(uint16_t, operation_ids);
    SWAP_FIELD(uint16_t, operation_deleted_row_masks);
    SWAP_FIELD(uint64_t, operation_masks);
#undef SWAP_FIELD
}

static void canonicalize_operation_set(
    ClearraPackingCandidateView *candidate) {
    for (uint8_t index = 1u; index < candidate->placed_count; ++index) {
        uint8_t cursor = index;
        while (cursor > 0u &&
               compare_operation_at(
                   candidate, (uint8_t)(cursor - 1u), cursor) > 0) {
            swap_operations(candidate, (uint8_t)(cursor - 1u), cursor);
            cursor--;
        }
    }
}

void clearra_packing_candidate_assign_geometry_representative(
    ClearraPackingCandidateView *candidate,
    uint8_t index,
    const ClearraPlacementCandidate *representative) {
    if (candidate == 0 || representative == 0 ||
        index >= CLEARRA_PACKING_MAX_PIECES) {
        return;
    }
    candidate->pieces[index] = representative->piece;
    candidate->rotations[index] = representative->rotation;
    candidate->xs[index] = representative->x;
    candidate->ys[index] = representative->y;
    candidate->operation_ids[index] = representative->operation_id;
    /* The complete deleted-row domain is restored by BuildUp. */
    candidate->operation_deleted_row_masks[index] = 0u;
    candidate->operation_masks[index] = representative->mask;
}

void clearra_packing_candidate_finalize_geometry(
    ClearraBoard64Layout layout,
    ClearraPackingCandidateView *candidate) {
    if (candidate == 0) {
        return;
    }
    canonicalize_operation_set(candidate);
    candidate->shape_key =
        clearra_packing_shape_key(layout, candidate->shape_mask);
    candidate->tiling_key = clearra_packing_geometry_tiling_key(
        layout,
        candidate->pieces,
        candidate->operation_masks,
        candidate->placed_count);
    candidate->operation_set_key = clearra_cache_key_mix_u64(
        candidate->tiling_key, UINT64_C(0x47454f4d444f4d4e));
}

static size_t max_candidate_rows(const clr_packing_problem *problem) {
    return problem->budget.max_results == 0u
        ? SIZE_MAX
        : (size_t)problem->budget.max_results;
}

static size_t max_total_bytes(const clr_packing_problem *problem) {
    if (problem->budget.has_max_memory_mib == 0u) {
        return SIZE_MAX;
    }
    uint64_t bytes = (uint64_t)problem->budget.max_memory_mib *
                     UINT64_C(1024) * UINT64_C(1024);
    return bytes > SIZE_MAX ? SIZE_MAX : (size_t)bytes;
}

static bool used_counts_match_problem(
    const clr_packing_problem *problem,
    const uint8_t used_piece_counts[CLR_STANDARD_PIECE_KIND_COUNT]) {
    if (problem->piece_multiset_family.count != 0u) {
        return clearra_piece_multiset_family_contains_exact(
            &problem->piece_multiset_family, used_piece_counts);
    }

    uint16_t used_total = 0u;
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; ++piece) {
        uint8_t used = used_piece_counts[piece];
        used_total = (uint16_t)(used_total + used);
        if (used != problem->piece_multiset_window.counts[piece]) {
            return false;
        }
    }
    return used_total == problem->piece_multiset_window.total_count &&
           (problem->piece_multiset_window.exact_count == 0u ||
            used_total == problem->piece_multiset_window.exact_count);
}

ClearraPackingStatus clearra_packing_materialize_catalog_path(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    const ClearraPackingGeometryPath *path,
    ClearraPackingCandidateView *out_candidate) {
    if (path == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    return clearra_packing_materialize_catalog_row_ids(
        catalog,
        problem,
        path->skeleton_ids,
        path->operation_count,
        out_candidate);
}

ClearraPackingStatus clearra_packing_materialize_catalog_row_ids(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    const uint32_t *skeleton_row_ids,
    uint8_t operation_count,
    ClearraPackingCandidateView *out_candidate) {
    if (!clearra_geometry_catalog_matches_problem(catalog, problem) ||
        skeleton_row_ids == 0 || out_candidate == 0 || operation_count == 0u ||
        operation_count > CLEARRA_PACKING_MAX_PIECES ||
        (problem->piece_window.has_exact_pieces != 0u &&
         operation_count != problem->piece_window.exact_pieces)) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }

    uint64_t occupied = problem->board.initial_mask;
    uint8_t used_piece_counts[CLR_STANDARD_PIECE_KIND_COUNT] = {0};
    clearra_packing_candidate_view_clear(out_candidate);
    out_candidate->placed_count = operation_count;
    for (uint8_t index = 0u; index < operation_count; ++index) {
        uint32_t skeleton_id = skeleton_row_ids[index];
        const ClearraInverseClearTemplate *representative =
            clearra_geometry_catalog_representative_template(
                catalog, skeleton_id);
        if (representative == 0 || skeleton_id >= catalog->skeleton_count) {
            return CLEARRA_PACKING_INVALID_ARGUMENT;
        }
        uint64_t mask = catalog->skeleton_cell_mask[skeleton_id];
        uint8_t piece = (uint8_t)catalog->skeleton_piece_kind[skeleton_id];
        if ((occupied & mask) != 0u ||
            (mask & problem->forbidden_mask) != 0u ||
            (mask & ~problem->goal_region_mask) != 0u ||
            (mask & ~problem->required_fill_mask) != 0u ||
            piece < CLR_PIECE_I || piece > CLR_PIECE_L) {
            return CLEARRA_PACKING_INVALID_ARGUMENT;
        }
        occupied |= mask;
        used_piece_counts[piece]++;
        ClearraPlacementCandidate placement = {
            .piece = piece,
            .rotation = representative->rotation,
            .x = representative->target_x,
            .y = representative->target_anchor_y,
            .operation_id = representative->operation_id,
            .required_deleted_row_mask =
                representative->minimum_deleted_row_mask,
            .mask = mask,
        };
        clearra_packing_candidate_assign_geometry_representative(
            out_candidate, index, &placement);
    }
    if (!used_counts_match_problem(problem, used_piece_counts) ||
        (problem->goal_region_mask & ~occupied) != 0u) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    ClearraBoard64LineClearResult clear_result;
    if (clearra_board64_clear_lines(
            catalog->layout, occupied, &clear_result) != CLEARRA_BOARD64_OK) {
        return CLEARRA_PACKING_INVALID_LAYOUT;
    }
    out_candidate->final_board = clear_result.board;
    out_candidate->shape_mask = occupied & ~problem->board.initial_mask;
    out_candidate->cleared_lines = clear_result.cleared_lines;
    out_candidate->geometry_variant_domains =
        (uint16_t)((UINT16_C(1) << operation_count) - UINT16_C(1));
    clearra_packing_candidate_finalize_geometry(catalog->layout, out_candidate);
    return CLEARRA_PACKING_OK;
}

ClearraPackingStatus
clearra_packing_materialize_catalog_paths_to_sink(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    const ClearraPackingGeometryPath *paths,
    uint32_t path_count,
    const ClearraPackingCandidateSink *sink,
    clr_resource_report *out_resource_report) {
    if (!clearra_geometry_catalog_matches_problem(catalog, problem) ||
        paths == 0 || path_count == 0u || sink == 0 || sink->consume == 0 ||
        out_resource_report == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    clr_resource_report_clear(out_resource_report);
    size_t accepted_count = 0u;
    size_t host_resident_bytes = 0u;
    uint32_t cancellation_counter = 0u;
    for (uint32_t path_index = 0u; path_index < path_count; ++path_index) {
        if (clr_execution_control_poll(&cancellation_counter)) {
            clr_resource_report_mark_truncated(
                out_resource_report, CLR_RESOURCE_TRUNCATION_CANCELLED);
            return CLEARRA_PACKING_CANCELLED;
        }
        ClearraPackingCandidateView candidate;
        ClearraPackingStatus status = clearra_packing_materialize_catalog_path(
            catalog, problem, &paths[path_index], &candidate);
        if (status != CLEARRA_PACKING_OK) {
            return status;
        }
        uint8_t inserted = 0u;
        uint16_t truncation_reason = CLR_RESOURCE_TRUNCATION_NONE;
        status = sink->consume(
            sink->context,
            &candidate,
            accepted_count,
            catalog->resident_bytes,
            max_candidate_rows(problem),
            max_total_bytes(problem),
            &inserted,
            &truncation_reason,
            &host_resident_bytes);
        if (status == CLEARRA_PACKING_CAPACITY_EXCEEDED) {
            clr_resource_report_mark_truncated(
                out_resource_report,
                truncation_reason == CLR_RESOURCE_TRUNCATION_NONE
                    ? CLR_RESOURCE_TRUNCATION_CANDIDATE_BUDGET_EXCEEDED
                    : truncation_reason);
            return status;
        }
        if (status != CLEARRA_PACKING_OK || inserted > 1u) {
            return status == CLEARRA_PACKING_OK
                ? CLEARRA_PACKING_INVALID_ARGUMENT
                : status;
        }
        accepted_count += inserted != 0u ? 1u : 0u;
        clr_resource_report_observe_candidate_rows(
            out_resource_report, accepted_count);
        clr_resource_report_observe_cpu_bytes(
            out_resource_report,
            catalog->resident_bytes > SIZE_MAX - host_resident_bytes
                ? SIZE_MAX
                : catalog->resident_bytes + host_resident_bytes);
    }
    return CLEARRA_PACKING_OK;
}
