#include "geometry_exact_cover_proof.h"

#include "../cache/cache_identity.h"

uint64_t clearra_geometry_catalog_identity_digest(
    const ClearraGeometryCatalogIdentity *identity) {
    uint64_t digest = UINT64_C(1469598103934665603);
    digest = clearra_cache_key_mix_u64(digest, identity->board_layout_id);
    digest = clearra_cache_key_mix_u64(
        digest, identity->compact_universe_digest);
    digest = clearra_cache_key_mix_u64(
        digest, identity->target_geometry_digest);
    digest = clearra_cache_key_mix_u64(digest, identity->piece_catalog_id);
    digest = clearra_cache_key_mix_u64(
        digest, identity->skeleton_projection_version);
    digest = clearra_cache_key_mix_u64(
        digest, identity->rule_capability_id);
    digest = clearra_cache_key_mix_u64(
        digest, identity->realization_table_digest);
    digest = clearra_cache_key_mix_u64(
        digest, identity->support_table_digest);
    return digest == 0u ? UINT64_C(1) : digest;
}

uint64_t clearra_geometry_search_batch_id(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    uint16_t family_begin,
    uint16_t family_end,
    uint16_t partition_index,
    uint16_t partition_count) {
    uint64_t digest = clearra_geometry_catalog_identity_digest(
        &catalog->identity);
    digest = clearra_cache_key_mix_u64(
        digest, problem->piece_source.piece_source_id);
    digest = clearra_cache_key_mix_u64(
        digest, problem->piece_source.pattern_universe_id);
    digest = clearra_cache_key_mix_u64(
        digest, problem->piece_source.pattern_weight_model_id);
    digest = clearra_cache_key_mix_u64(
        digest, ((uint64_t)family_begin << 48u) |
                    ((uint64_t)family_end << 32u) |
                    ((uint64_t)partition_index << 16u) | partition_count);
    return digest == 0u ? UINT64_C(1) : digest;
}

static bool producer_authorizes_reason(
    clr_pruning_producer_id producer_id,
    clr_prune_reason prune_reason) {
    switch (producer_id) {
        case CLR_PRUNING_PRODUCER_GEOMETRY_FULL_PLACEMENT_DOMAIN:
            return prune_reason == CLR_PRUNE_FULL_PARENT_DOMAIN_EMPTY ||
                   prune_reason == CLR_PRUNE_SAME_TILE_PARENT_DOMAIN_EMPTY;
        case CLR_PRUNING_PRODUCER_GEOMETRY_APDP:
            return prune_reason == CLR_PRUNE_SAME_TILE_PARENT_DOMAIN_EMPTY;
        case CLR_PRUNING_PRODUCER_GEOMETRY_HALL_BOUND:
            return prune_reason == CLR_PRUNE_PARENT_DOMAIN_HALL_VIOLATION;
        case CLR_PRUNING_PRODUCER_GEOMETRY_COLUMN_PROJECTION:
            return prune_reason == CLR_PRUNE_COLUMN_DEMAND_OVERFLOW ||
                   prune_reason == CLR_PRUNE_COLUMN_DEMAND_UNREACHABLE;
        case CLR_PRUNING_PRODUCER_GEOMETRY_ADDITIVE_INVARIANT:
            return prune_reason == CLR_PRUNE_ADDITIVE_INVARIANT_MISMATCH;
        case CLR_PRUNING_PRODUCER_GEOMETRY_COMPONENT_DECOMPOSITION:
            return prune_reason == CLR_PRUNE_SEPARATOR_COMPONENT_INFEASIBLE;
        case CLR_PRUNING_PRODUCER_GEOMETRY_BUMPER_DOMAIN:
            return prune_reason == CLR_PRUNE_BUMPER_DOMAIN_EMPTY ||
                   prune_reason == CLR_PRUNE_BUMPER_BRIDGE_INCOMPATIBLE;
        default:
            return false;
    }
}

static ClearraPackingStatus authorize_engine_proof(
    ClearraGeometryExactCoverSearch *search,
    uint8_t state_layer,
    clr_pruning_producer_id producer_id,
    clr_prune_reason prune_reason,
    uint64_t evidence_digest,
    uint32_t affected_candidate_count,
    bool *out_authorized) {
    if (search == 0 || search->pruning_ledger == 0 ||
        producer_id == 0 || evidence_digest == 0u ||
        !producer_authorizes_reason(producer_id, prune_reason) ||
        affected_candidate_count == 0u || out_authorized == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    *out_authorized = false;
    clr_pruning_proof_ledger_entry entry = {
        .batch_id = search->pruning_batch_id,
        .producer_id = producer_id,
        .catalog_identity_digest = search->pruning_catalog_identity_digest,
        .state_layer = state_layer,
        .prune_reason = prune_reason,
        .proof_level = CLR_PRUNE_PROOF_GLOBAL_SAFE,
        .fallback_if_invalid = CLR_PRUNE_FALLBACK_KEEP_CANDIDATE,
        .affected_candidate_count = affected_candidate_count,
        .evidence_digest = evidence_digest,
    };
    clr_pruning_status status = clr_pruning_proof_ledger_record(
        search->pruning_ledger, entry);
    if (status == CLR_PRUNING_OK) {
        *out_authorized = true;
        return CLEARRA_PACKING_OK;
    }
    return status == CLR_PRUNING_EVIDENCE_CAPACITY_UNAVAILABLE
        ? CLEARRA_PACKING_OK
        : CLEARRA_PACKING_INVALID_ARGUMENT;
}

ClearraPackingStatus clearra_geometry_authorize_full_placement_domain(
    ClearraGeometryExactCoverSearch *search,
    uint8_t state_layer,
    ClearraGeometryDomainStatus status,
    const ClearraGeometryDomainPropagation *result,
    bool *out_authorized) {
    if (result == 0 || out_authorized == 0 ||
        (status != CLEARRA_GEOMETRY_DOMAIN_EMPTY &&
         !(status == CLEARRA_GEOMETRY_DOMAIN_SUPPORTED &&
           result->pivot_filtered_row_count != 0u))) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    clr_prune_reason reason = status == CLEARRA_GEOMETRY_DOMAIN_EMPTY &&
            result->same_tile_certificate_count == 0u
        ? CLR_PRUNE_FULL_PARENT_DOMAIN_EMPTY
        : CLR_PRUNE_SAME_TILE_PARENT_DOMAIN_EMPTY;
    return authorize_engine_proof(
        search,
        state_layer,
        CLR_PRUNING_PRODUCER_GEOMETRY_FULL_PLACEMENT_DOMAIN,
        reason,
        result->evidence_digest,
        result->pivot_filtered_row_count == 0u
            ? 1u
            : result->pivot_filtered_row_count,
        out_authorized);
}

ClearraPackingStatus clearra_geometry_authorize_bumper_domain(
    ClearraGeometryExactCoverSearch *search,
    uint8_t state_layer,
    ClearraGeometryBumperStatus status,
    const ClearraGeometryBumperResult *result,
    bool *out_authorized) {
    if (result == 0 || out_authorized == 0 ||
        (status != CLEARRA_GEOMETRY_BUMPER_EMPTY &&
         !(status == CLEARRA_GEOMETRY_BUMPER_SUPPORTED &&
           result->filtered_parent_row_count != 0u))) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    return authorize_engine_proof(
        search,
        state_layer,
        CLR_PRUNING_PRODUCER_GEOMETRY_BUMPER_DOMAIN,
        status == CLEARRA_GEOMETRY_BUMPER_EMPTY
            ? CLR_PRUNE_BUMPER_DOMAIN_EMPTY
            : CLR_PRUNE_BUMPER_BRIDGE_INCOMPATIBLE,
        result->evidence_digest,
        result->filtered_parent_row_count == 0u
            ? 1u
            : result->filtered_parent_row_count,
        out_authorized);
}

ClearraPackingStatus clearra_geometry_authorize_apdp_domain(
    ClearraGeometryExactCoverSearch *search,
    uint8_t state_layer,
    ClearraGeometryApdpStatus status,
    const ClearraGeometryApdpResult *result,
    bool *out_authorized) {
    if (result == 0 || out_authorized == 0 ||
        (status != CLEARRA_GEOMETRY_APDP_EMPTY &&
         !(status == CLEARRA_GEOMETRY_APDP_SUPPORTED &&
           result->filtered_parent_row_count != 0u))) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    return authorize_engine_proof(
        search,
        state_layer,
        CLR_PRUNING_PRODUCER_GEOMETRY_APDP,
        CLR_PRUNE_SAME_TILE_PARENT_DOMAIN_EMPTY,
        result->exact_parent_row_digest,
        result->filtered_parent_row_count == 0u
            ? 1u
            : result->filtered_parent_row_count,
        out_authorized);
}

ClearraPackingStatus clearra_geometry_authorize_hall_bound(
    ClearraGeometryExactCoverSearch *search,
    uint8_t state_layer,
    ClearraGeometryHallStatus status,
    const ClearraGeometryHallResult *result,
    bool *out_authorized) {
    if (status != CLEARRA_GEOMETRY_HALL_IMPOSSIBLE || result == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    return authorize_engine_proof(
        search,
        state_layer,
        CLR_PRUNING_PRODUCER_GEOMETRY_HALL_BOUND,
        CLR_PRUNE_PARENT_DOMAIN_HALL_VIOLATION,
        result->evidence_digest,
        1u,
        out_authorized);
}

ClearraPackingStatus clearra_geometry_authorize_column_projection(
    ClearraGeometryExactCoverSearch *search,
    uint8_t state_layer,
    ClearraGeometryColumnProjectionStatus status,
    const ClearraGeometryColumnProjectionResult *result,
    bool *out_authorized) {
    if (status != CLEARRA_GEOMETRY_COLUMN_PROJECTION_IMPOSSIBLE ||
        result == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    return authorize_engine_proof(
        search,
        state_layer,
        CLR_PRUNING_PRODUCER_GEOMETRY_COLUMN_PROJECTION,
        CLR_PRUNE_COLUMN_DEMAND_OVERFLOW,
        result->evidence_digest,
        1u,
        out_authorized);
}

ClearraPackingStatus clearra_geometry_authorize_projection_reachability(
    ClearraGeometryExactCoverSearch *search,
    uint8_t state_layer,
    ClearraGeometryProjectionReachabilityStatus status,
    const ClearraGeometryProjectionReachabilityResult *result,
    bool *out_authorized) {
    if (status != CLEARRA_GEOMETRY_PROJECTION_UNREACHABLE || result == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    return authorize_engine_proof(
        search,
        state_layer,
        CLR_PRUNING_PRODUCER_GEOMETRY_COLUMN_PROJECTION,
        CLR_PRUNE_COLUMN_DEMAND_UNREACHABLE,
        result->evidence_digest,
        result->checked_piece_count_vectors == 0u
            ? 1u
            : result->checked_piece_count_vectors,
        out_authorized);
}

ClearraPackingStatus clearra_geometry_authorize_additive_invariant(
    ClearraGeometryExactCoverSearch *search,
    uint8_t state_layer,
    ClearraGeometryInvariantStatus status,
    const ClearraGeometryInvariantResult *result,
    bool *out_authorized) {
    if (status != CLEARRA_GEOMETRY_INVARIANT_IMPOSSIBLE || result == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    return authorize_engine_proof(
        search,
        state_layer,
        CLR_PRUNING_PRODUCER_GEOMETRY_ADDITIVE_INVARIANT,
        CLR_PRUNE_ADDITIVE_INVARIANT_MISMATCH,
        result->evidence_digest,
        1u,
        out_authorized);
}

static uint64_t component_infeasible_evidence_digest(
    uint64_t remaining_cells,
    const ClearraGeometryComponentDecomposition *decomposition,
    uint64_t producer_discriminator) {
    uint64_t digest = UINT64_C(1469598103934665603);
    digest = clearra_cache_key_mix_u64(digest, remaining_cells);
    digest = clearra_cache_key_mix_u64(
        digest, decomposition->unsupported_cells);
    digest = clearra_cache_key_mix_u64(
        digest, decomposition->component_count);
    for (uint8_t index = 0u; index < decomposition->component_count; ++index) {
        digest = clearra_cache_key_mix_u64(
            digest, decomposition->component_masks[index]);
    }
    digest = clearra_cache_key_mix_u64(digest, producer_discriminator);
    return digest == 0u ? UINT64_C(1) : digest;
}

ClearraPackingStatus clearra_geometry_authorize_component_infeasible(
    ClearraGeometryExactCoverSearch *search,
    uint8_t state_layer,
    uint64_t remaining_cells,
    const ClearraGeometryComponentDecomposition *decomposition,
    uint64_t producer_discriminator,
    bool *out_authorized) {
    if (decomposition == 0 ||
        decomposition->component_count > CLEARRA_GEOMETRY_MAX_COMPONENTS ||
        (decomposition->component_count == 0u &&
         decomposition->unsupported_cells == 0u) ||
        out_authorized == 0) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    return authorize_engine_proof(
        search,
        state_layer,
        CLR_PRUNING_PRODUCER_GEOMETRY_COMPONENT_DECOMPOSITION,
        CLR_PRUNE_SEPARATOR_COMPONENT_INFEASIBLE,
        component_infeasible_evidence_digest(
            remaining_cells, decomposition, producer_discriminator),
        1u,
        out_authorized);
}
