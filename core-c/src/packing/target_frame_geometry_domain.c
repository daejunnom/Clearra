#include "packing_problem.h"

typedef struct ClearraTargetFrameGeometryDomain {
    ClearraBoard64Layout layout;
    uint64_t board;
    uint64_t target_mask;
    const clr_static_prune_context *base_prune_context;
    clr_pruning_proof_ledger *ledger;
    ClearraPlacementCandidateVisitor visitor;
    void *visitor_context;
    const ClearraOperation *operation;
    int8_t x;
    uint8_t local_rows[CLEARRA_TETROMINO_AREA];
    uint8_t target_rows[CLEARRA_TETROMINO_AREA];
    uint8_t row_count;
} ClearraTargetFrameGeometryDomain;

static ClearraPackingStatus clearra_placement_candidate_list_visitor(
    void *context,
    const ClearraPlacementCandidate *candidate);

/*
 * This is the minimum deleted-row requirement for the row projection, not an
 * exact clear state. Rows outside the projected operation span may already be
 * deleted without changing the canonical ownership. Exact compatibility is
 * confirmed by replaying the projection in geometry_realization_domain.c.
 */
static uint16_t minimum_required_deleted_rows(
    const ClearraTargetFrameGeometryDomain *domain) {
    uint16_t deleted = 0u;
    for (uint8_t index = 1u; index < domain->row_count; ++index) {
        uint8_t local_gap = (uint8_t)(
            domain->local_rows[index] - domain->local_rows[index - 1u]);
        uint8_t first_deleted =
            (uint8_t)(domain->target_rows[index - 1u] + local_gap);
        for (uint8_t row = first_deleted;
             row < domain->target_rows[index];
             ++row) {
            deleted = (uint16_t)(deleted | (uint16_t)(UINT16_C(1) << row));
        }
    }
    return deleted;
}

static ClearraPackingStatus packing_status_from_operation_status(
    ClearraOperationStatus status) {
    if (status == CLEARRA_OPERATION_OK) {
        return CLEARRA_PACKING_OK;
    }
    if (status == CLEARRA_OPERATION_INVALID_PIECE) {
        return CLEARRA_PACKING_INVALID_PIECE;
    }
    if (status == CLEARRA_OPERATION_OUT_OF_BOUNDS) {
        return CLEARRA_PACKING_OUT_OF_BOUNDS;
    }
    return CLEARRA_PACKING_INVALID_ARGUMENT;
}

static uint8_t operation_local_rows(
    const ClearraOperation *operation,
    uint8_t out_rows[CLEARRA_TETROMINO_AREA]) {
    uint8_t count = 0u;
    for (uint8_t cell = 0u; cell < operation->area; ++cell) {
        uint8_t row = (uint8_t)operation->cells[cell].y;
        uint8_t insert_at = 0u;
        while (insert_at < count && out_rows[insert_at] < row) {
            ++insert_at;
        }
        if (insert_at < count && out_rows[insert_at] == row) {
            continue;
        }
        for (uint8_t cursor = count; cursor > insert_at; --cursor) {
            out_rows[cursor] = out_rows[cursor - 1u];
        }
        out_rows[insert_at] = row;
        ++count;
    }
    return count;
}

static uint8_t target_row_for_local_row(
    const ClearraTargetFrameGeometryDomain *domain,
    uint8_t local_row) {
    for (uint8_t index = 0u; index < domain->row_count; ++index) {
        if (domain->local_rows[index] == local_row) {
            return domain->target_rows[index];
        }
    }
    return UINT8_MAX;
}

static ClearraPackingStatus emit_projection(
    ClearraTargetFrameGeometryDomain *domain) {
    uint64_t mask = 0u;
    for (uint8_t cell = 0u; cell < domain->operation->area; ++cell) {
        uint8_t target_y = target_row_for_local_row(
            domain, (uint8_t)domain->operation->cells[cell].y);
        uint8_t target_x =
            (uint8_t)(domain->x + domain->operation->cells[cell].x);
        uint64_t cell_mask = 0u;
        if (target_y == UINT8_MAX ||
            clearra_board64_cell_mask(
                domain->layout, target_x, target_y, &cell_mask) !=
                CLEARRA_BOARD64_OK) {
            return CLEARRA_PACKING_OUT_OF_BOUNDS;
        }
        mask |= cell_mask;
    }

    clr_static_prune_context prune_context = *domain->base_prune_context;
    prune_context.piece = domain->operation->piece;
    prune_context.rotation = domain->operation->rotation;
    prune_context.x = domain->x;
    prune_context.y = (int8_t)domain->target_rows[0];
    prune_context.operation_id = domain->operation->operation_id;
    bool accepts = false;
    ClearraPackingStatus status =
        clearra_packing_pruner_accepts_static_candidate_with_ledger(
            domain->layout,
            domain->board,
            domain->target_mask,
            mask,
            &prune_context,
            domain->ledger,
            &accepts);
    if (status != CLEARRA_PACKING_OK || !accepts) {
        return status;
    }

    ClearraPlacementCandidate candidate = {
        .piece = domain->operation->piece,
        .rotation = domain->operation->rotation,
        .x = domain->x,
        .y = (int8_t)domain->target_rows[0],
        .operation_id = domain->operation->operation_id,
        .required_deleted_row_mask = minimum_required_deleted_rows(domain),
        .mask = mask,
    };
    return domain->visitor(domain->visitor_context, &candidate);
}

static ClearraPackingStatus enumerate_row_projections(
    ClearraTargetFrameGeometryDomain *domain,
    uint8_t row_index) {
    if (row_index == domain->row_count) {
        return emit_projection(domain);
    }

    uint8_t local_row = domain->local_rows[row_index];
    uint8_t local_last = domain->local_rows[domain->row_count - 1u];
    uint8_t minimum = local_row;
    if (row_index > 0u) {
        uint8_t local_gap =
            (uint8_t)(local_row - domain->local_rows[row_index - 1u]);
        minimum = (uint8_t)(domain->target_rows[row_index - 1u] + local_gap);
    }
    uint8_t remaining_minimum_span = (uint8_t)(local_last - local_row);
    if (remaining_minimum_span >= domain->layout.height) {
        return CLEARRA_PACKING_OK;
    }
    uint8_t maximum =
        (uint8_t)(domain->layout.height - 1u - remaining_minimum_span);
    if (minimum > maximum) {
        return CLEARRA_PACKING_OK;
    }

    for (uint8_t target_row = minimum; target_row <= maximum; ++target_row) {
        domain->target_rows[row_index] = target_row;
        ClearraPackingStatus status =
            enumerate_row_projections(domain, (uint8_t)(row_index + 1u));
        if (status != CLEARRA_PACKING_OK) {
            return status;
        }
    }
    return CLEARRA_PACKING_OK;
}

ClearraPackingStatus clearra_placement_candidates_generate(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint64_t target_mask,
    uint8_t piece,
    ClearraPlacementCandidateList *out_list) {
    clr_static_prune_context context;
    clr_pruning_proof_ledger ledger;
    ClearraPackingStatus status = clearra_packing_prune_context_for_geometry(
        layout, board, target_mask, &context);
    if (status != CLEARRA_PACKING_OK) {
        return status;
    }
    clr_pruning_proof_ledger_init(&ledger);
    return clearra_placement_candidates_generate_with_pruning_ledger(
        layout, board, target_mask, piece, &context, &ledger, out_list);
}

ClearraPackingStatus clearra_placement_candidates_generate_with_pruning_ledger(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint64_t target_mask,
    uint8_t piece,
    const clr_static_prune_context *base_prune_context,
    clr_pruning_proof_ledger *ledger,
    ClearraPlacementCandidateList *out_list) {
    if (out_list == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    clearra_placement_candidate_list_clear(out_list);
    return clearra_placement_candidates_visit_with_pruning_ledger(
        layout,
        board,
        target_mask,
        piece,
        base_prune_context,
        ledger,
        clearra_placement_candidate_list_visitor,
        out_list);
}

static ClearraPackingStatus clearra_placement_candidate_list_visitor(
    void *context,
    const ClearraPlacementCandidate *candidate) {
    if (context == 0 || candidate == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    return clearra_placement_candidate_list_push(
        (ClearraPlacementCandidateList *)context, *candidate);
}

ClearraPackingStatus clearra_placement_candidates_visit_with_pruning_ledger(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint64_t target_mask,
    uint8_t piece,
    const clr_static_prune_context *base_prune_context,
    clr_pruning_proof_ledger *ledger,
    ClearraPlacementCandidateVisitor visitor,
    void *visitor_context) {
    if (ledger == 0 || visitor == 0 ||
        !clearra_packing_prune_context_is_valid(base_prune_context)) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    if (!clearra_board64_layout_is_valid(layout) || layout.height > 16u) {
        return CLEARRA_PACKING_INVALID_LAYOUT;
    }
    if (!clearra_piece_is_standard_tetromino(piece)) {
        return CLEARRA_PACKING_INVALID_PIECE;
    }
    for (uint8_t rotation = 0u;
         rotation < clearra_rotation_count_for_piece(piece);
         ++rotation) {
        ClearraOperation operation;
        ClearraOperationStatus operation_status =
            clearra_operation_from_shape(piece, rotation, &operation);
        if (operation_status != CLEARRA_OPERATION_OK) {
            return packing_status_from_operation_status(operation_status);
        }
        int16_t max_x =
            (int16_t)layout.width - (int16_t)operation.bounds.width;
        if (max_x < 0) {
            continue;
        }
        for (int16_t x = 0; x <= max_x; ++x) {
            ClearraTargetFrameGeometryDomain domain = {
                .layout = layout,
                .board = board,
                .target_mask = target_mask,
                .base_prune_context = base_prune_context,
                .ledger = ledger,
                .visitor = visitor,
                .visitor_context = visitor_context,
                .operation = &operation,
                .x = (int8_t)x,
            };
            domain.row_count =
                operation_local_rows(&operation, domain.local_rows);
            if (domain.row_count == 0u) {
                return CLEARRA_PACKING_INVALID_ARGUMENT;
            }
            ClearraPackingStatus status =
                enumerate_row_projections(&domain, 0u);
            if (status != CLEARRA_PACKING_OK) {
                return status;
            }
        }
    }
    return CLEARRA_PACKING_OK;
}
