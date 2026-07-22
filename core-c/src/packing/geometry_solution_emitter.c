#include "geometry_exact_cover_internal.h"

#include "packing_candidate_materializer.h"
#include "clr_execution_control.h"

static ClearraPlacementCandidate representative_placement(
    const ClearraInverseClearTemplate *template_value) {
    return (ClearraPlacementCandidate){
        .piece = template_value->piece,
        .rotation = template_value->rotation,
        .x = template_value->target_x,
        .y = template_value->target_anchor_y,
        .operation_id = template_value->operation_id,
        .required_deleted_row_mask =
            template_value->minimum_deleted_row_mask,
        .mask = template_value->canonical_cell_ownership,
    };
}

static ClearraPackingStatus materialize_candidate(
    ClearraGeometryExactCoverSearch *search,
    ClearraPackingCandidateView *out_candidate) {
    clearra_packing_candidate_view_clear(out_candidate);
    out_candidate->placed_count = search->target_depth;
    uint64_t occupied = search->catalog->initial_board;
    for (uint8_t index = 0u; index < search->target_depth; ++index) {
        uint32_t row_id = search->selected_rows[index];
        const ClearraInverseClearTemplate *template_value =
            clearra_geometry_catalog_representative_template(
                search->catalog, row_id);
        if (template_value == 0) {
            return CLEARRA_PACKING_INVALID_ARGUMENT;
        }
        ClearraPlacementCandidate placement =
            representative_placement(template_value);
        clearra_packing_candidate_assign_geometry_representative(
            out_candidate, index, &placement);
        occupied |= search->catalog->skeleton_cell_mask[row_id];
    }
    ClearraBoard64LineClearResult clear_result;
    if (clearra_board64_clear_lines(
            search->catalog->layout, occupied, &clear_result) !=
        CLEARRA_BOARD64_OK) {
        return CLEARRA_PACKING_INVALID_LAYOUT;
    }
    out_candidate->final_board = clear_result.board;
    out_candidate->shape_mask = occupied & ~search->catalog->initial_board;
    out_candidate->cleared_lines = clear_result.cleared_lines;
    out_candidate->geometry_variant_domains = (uint16_t)(
        (UINT16_C(1) << search->target_depth) - UINT16_C(1));
    clearra_packing_candidate_finalize_geometry(
        search->catalog->layout, out_candidate);
    return CLEARRA_PACKING_OK;
}

static ClearraPackingStatus accept_candidate(
    ClearraGeometryExactCoverSearch *search,
    const ClearraPackingCandidateView *candidate) {
    bool inserted = false;
    if (search->output.buffer != 0) {
        ClearraPackingStatus status = clearra_packing_deduper_push_unique(
            search->output.buffer, candidate, 0, &inserted);
        if (status != CLEARRA_PACKING_OK) {
            clr_resource_report_mark_truncated(
                search->resource_report,
                status == CLEARRA_PACKING_CAPACITY_EXCEEDED
                    ? CLR_RESOURCE_TRUNCATION_CANDIDATE_BUDGET_EXCEEDED
                    : CLR_RESOURCE_TRUNCATION_MEMORY_EXCEEDED);
            return status;
        }
        if (inserted &&
            search->output.accepted_count >= search->output.max_candidate_rows) {
            search->output.buffer->count--;
            clr_resource_report_mark_truncated(
                search->resource_report,
                CLR_RESOURCE_TRUNCATION_CANDIDATE_BUDGET_EXCEEDED);
            clr_resource_report_observe_candidate_rows(
                search->resource_report,
                search->output.accepted_count + 1u);
            return CLEARRA_PACKING_CAPACITY_EXCEEDED;
        }
    } else {
        uint8_t sink_inserted = 0u;
        uint16_t reason = CLR_RESOURCE_TRUNCATION_NONE;
        size_t host_bytes = search->output.host_resident_bytes;
        size_t engine_resident_bytes =
            clearra_geometry_search_resident_bytes(search) -
            search->output.host_resident_bytes;
        ClearraPackingStatus status = search->output.sink->consume(
            search->output.sink->context,
            candidate,
            search->output.accepted_count,
            engine_resident_bytes,
            search->output.max_candidate_rows,
            search->output.max_total_bytes,
            &sink_inserted,
            &reason,
            &host_bytes);
        if (status != CLEARRA_PACKING_OK || sink_inserted > 1u) {
            if (status == CLEARRA_PACKING_CAPACITY_EXCEEDED) {
                clr_resource_report_mark_truncated(
                    search->resource_report,
                    reason == CLR_RESOURCE_TRUNCATION_NONE
                        ? CLR_RESOURCE_TRUNCATION_CANDIDATE_BUDGET_EXCEEDED
                        : reason);
            }
            return status == CLEARRA_PACKING_OK
                ? CLEARRA_PACKING_INVALID_ARGUMENT
                : status;
        }
        search->output.host_resident_bytes = host_bytes;
        inserted = sink_inserted != 0u;
    }
    if (inserted) {
        search->output.accepted_count++;
    }
    clr_resource_report_observe_candidate_rows(
        search->resource_report, search->output.accepted_count);
    clr_resource_report_observe_cpu_bytes(
        search->resource_report,
        clearra_geometry_search_resident_bytes(search));
    return CLEARRA_PACKING_OK;
}

static ClearraPackingStatus emit_with_continuations(
    ClearraGeometryExactCoverSearch *search,
    ClearraGeometryFamilyRef family,
    uint8_t depth,
    ClearraGeometryFamilyRef continuations[CLEARRA_PACKING_MAX_PIECES],
    uint8_t continuation_count) {
    if (family == CLEARRA_GEOMETRY_FAMILY_INVALID) {
        return CLEARRA_PACKING_OK;
    }
    if (clr_execution_control_poll(&search->cancellation_poll_counter)) {
        clr_resource_report_mark_truncated(
            search->resource_report, CLR_RESOURCE_TRUNCATION_CANCELLED);
        return CLEARRA_PACKING_CANCELLED;
    }
    if (family == CLEARRA_GEOMETRY_FAMILY_EMPTY) {
        if (continuation_count != 0u) {
            return emit_with_continuations(
                search,
                continuations[continuation_count - 1u],
                depth,
                continuations,
                (uint8_t)(continuation_count - 1u));
        }
        if (depth != search->target_depth) {
            return CLEARRA_PACKING_INVALID_ARGUMENT;
        }
        ClearraPackingCandidateView candidate;
        ClearraPackingStatus status = materialize_candidate(search, &candidate);
        return status == CLEARRA_PACKING_OK
            ? accept_candidate(search, &candidate)
            : status;
    }
    const ClearraGeometryFamilyNode *node =
        clearra_geometry_solution_family_node(&search->solution_family, family);
    if (node == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    if (node->kind == CLEARRA_GEOMETRY_FAMILY_APPEND) {
        if (depth >= search->target_depth ||
            node->row_id >= search->catalog->skeleton_count) {
            return CLEARRA_PACKING_INVALID_ARGUMENT;
        }
        search->selected_rows[depth] = node->row_id;
        return emit_with_continuations(
            search,
            node->left,
            (uint8_t)(depth + 1u),
            continuations,
            continuation_count);
    }
    if (node->kind == CLEARRA_GEOMETRY_FAMILY_PRODUCT) {
        if (continuation_count >= CLEARRA_PACKING_MAX_PIECES) {
            return CLEARRA_PACKING_INVALID_ARGUMENT;
        }
        continuations[continuation_count] = node->right;
        return emit_with_continuations(
            search,
            node->left,
            depth,
            continuations,
            (uint8_t)(continuation_count + 1u));
    }
    if (node->kind != CLEARRA_GEOMETRY_FAMILY_UNION) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    ClearraPackingStatus status = emit_with_continuations(
        search,
        node->left,
        depth,
        continuations,
        continuation_count);
    return status == CLEARRA_PACKING_OK
        ? emit_with_continuations(
              search,
              node->right,
              depth,
              continuations,
              continuation_count)
        : status;
}

ClearraPackingStatus clearra_geometry_emit_solution_family(
    ClearraGeometryExactCoverSearch *search,
    ClearraGeometryFamilyRef family,
    uint8_t depth) {
    ClearraGeometryFamilyRef continuations[CLEARRA_PACKING_MAX_PIECES];
    return emit_with_continuations(
        search, family, depth, continuations, 0u);
}
