#include "packing_problem.h"
static uint64_t static_filter_catalog_digest(
    const clr_static_prune_context *context) {
    uint64_t hash = UINT64_C(1469598103934665603);
    hash ^= context->operation_table_version;
    hash *= UINT64_C(1099511628211);
    hash ^= context->piece_set_id;
    hash *= UINT64_C(1099511628211);
    hash ^= context->rule_profile_id;
    hash *= UINT64_C(1099511628211);
    hash ^= context->kick_profile_id;
    hash *= UINT64_C(1099511628211);
    return hash == 0u ? UINT64_C(1) : hash;
}

static uint64_t static_filter_evidence_digest(
    uint64_t occupied_board,
    uint64_t target_mask,
    uint64_t placement_mask,
    clr_prune_reason reason,
    const clr_static_prune_context *context) {
    uint64_t hash = UINT64_C(1469598103934665603);
    hash ^= occupied_board;
    hash *= UINT64_C(1099511628211);
    hash ^= target_mask;
    hash *= UINT64_C(1099511628211);
    hash ^= placement_mask;
    hash *= UINT64_C(1099511628211);
    hash ^= (uint64_t)reason;
    hash *= UINT64_C(1099511628211);
    hash ^= context->batch_id;
    hash *= UINT64_C(1099511628211);
    hash ^= (uint64_t)context->state_layer;
    hash *= UINT64_C(1099511628211);
    hash ^= (uint64_t)context->piece;
    hash *= UINT64_C(1099511628211);
    hash ^= (uint64_t)context->rotation;
    hash *= UINT64_C(1099511628211);
    hash ^= (uint64_t)(uint8_t)context->x;
    hash *= UINT64_C(1099511628211);
    hash ^= (uint64_t)(uint8_t)context->y;
    hash *= UINT64_C(1099511628211);
    hash ^= (uint64_t)context->operation_id;
    hash *= UINT64_C(1099511628211);
    hash ^= context->operation_table_version;
    hash *= UINT64_C(1099511628211);
    hash ^= context->piece_set_id;
    hash *= UINT64_C(1099511628211);
    hash ^= context->rule_profile_id;
    hash *= UINT64_C(1099511628211);
    hash ^= context->kick_profile_id;
    hash *= UINT64_C(1099511628211);
    return hash == 0u ? UINT64_C(1) : hash;
}

static ClearraPackingStatus record_static_candidate_drop(
    clr_pruning_proof_ledger *ledger,
    uint64_t occupied_board,
    uint64_t target_mask,
    uint64_t placement_mask,
    clr_prune_reason reason,
    const clr_static_prune_context *context,
    bool *out_keep_candidate) {
    if (!clearra_packing_prune_context_is_valid(context) || ledger == 0 ||
        out_keep_candidate == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    *out_keep_candidate = false;

    clr_pruning_proof_ledger_entry entry = {0};
    entry.batch_id = context->batch_id;
    entry.producer_id = CLR_PRUNING_PRODUCER_STATIC_PLACEMENT_FILTER;
    entry.catalog_identity_digest = static_filter_catalog_digest(context);
    entry.state_layer = context->state_layer;
    entry.prune_reason = (uint8_t)reason;
    entry.proof_level = (uint8_t)CLR_PRUNE_PROOF_GLOBAL_SAFE;
    entry.fallback_if_invalid = (uint8_t)CLR_PRUNE_FALLBACK_RUN_BUILDUP;
    entry.affected_candidate_count = 1u;
    if (reason != CLR_PRUNE_PLACEMENT_COLLISION &&
        reason != CLR_PRUNE_TARGET_MASK_OVERFLOW) {
        *out_keep_candidate = true;
        return CLEARRA_PACKING_OK;
    }
    entry.evidence_digest = static_filter_evidence_digest(
        occupied_board,
        target_mask,
        placement_mask,
        reason,
        context);
    clr_pruning_status status = clr_pruning_proof_ledger_record(ledger, entry);
    if (status == CLR_PRUNING_OK) {
        return CLEARRA_PACKING_OK;
    }
    if (status == CLR_PRUNING_EVIDENCE_CAPACITY_UNAVAILABLE) {
        *out_keep_candidate = true;
        return CLEARRA_PACKING_OK;
    }
    return CLEARRA_PACKING_INVALID_ARGUMENT;
}

ClearraPackingStatus clearra_packing_target_mask_for_lines(
    ClearraBoard64Layout layout,
    uint8_t target_lines,
    uint64_t *out_mask) {
    if (out_mask == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    if (!clearra_board64_layout_is_valid(layout) || target_lines == 0 ||
        target_lines > layout.height) {
        return CLEARRA_PACKING_INVALID_LAYOUT;
    }

    uint64_t mask = 0;
    for (uint8_t y = 0; y < target_lines; y++) {
        uint64_t row_mask = 0;
        ClearraBoard64Status status = clearra_board64_row_mask(layout, y, &row_mask);
        if (status != CLEARRA_BOARD64_OK) {
            return CLEARRA_PACKING_INVALID_LAYOUT;
        }
        mask |= row_mask;
    }

    *out_mask = mask;
    return CLEARRA_PACKING_OK;
}

ClearraPackingStatus clearra_packing_pruner_accepts_static_candidate_with_ledger(
    ClearraBoard64Layout layout,
    uint64_t occupied_board,
    uint64_t target_mask,
    uint64_t placement_mask,
    const clr_static_prune_context *context,
    clr_pruning_proof_ledger *ledger,
    bool *out_accepts) {
    if (out_accepts == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    *out_accepts = false;
    if (!clearra_packing_prune_context_is_valid(context) || ledger == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    if (!clearra_board64_layout_is_valid(layout)) {
        return CLEARRA_PACKING_INVALID_LAYOUT;
    }
    if ((occupied_board | target_mask | placement_mask) & ~layout.all_cells_mask) {
        return CLEARRA_PACKING_OUT_OF_BOUNDS;
    }
    if ((placement_mask & ~target_mask) != 0) {
        return record_static_candidate_drop(
            ledger,
            occupied_board,
            target_mask,
            placement_mask,
            CLR_PRUNE_TARGET_MASK_OVERFLOW,
            context,
            out_accepts);
    }

    bool collision = false;
    ClearraBoard64Status status =
        clearra_board64_collision(layout, occupied_board, placement_mask, &collision);
    if (status != CLEARRA_BOARD64_OK) {
        return CLEARRA_PACKING_INVALID_LAYOUT;
    }
    if (collision) {
        return record_static_candidate_drop(
            ledger,
            occupied_board,
            target_mask,
            placement_mask,
            CLR_PRUNE_PLACEMENT_COLLISION,
            context,
            out_accepts);
    }
    *out_accepts = true;
    return CLEARRA_PACKING_OK;
}
