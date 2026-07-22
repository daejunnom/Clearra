#include "packing_problem.h"
static uint64_t nonzero_identity(uint64_t identity) {
    return identity == 0u ? UINT64_C(1) : identity;
}bool clearra_packing_prune_context_is_valid(
    const clr_static_prune_context *context) {
    return context != 0 && context->batch_id != 0u &&
           context->operation_table_version != 0u &&
           context->piece_set_id != 0u && context->rule_profile_id != 0u &&
           context->kick_profile_id != 0u;
}ClearraPackingStatus clearra_packing_prune_context_from_problem(
    const clr_packing_problem *problem,
    clr_static_prune_context *out_context) {
    if (problem == 0 || out_context == 0 ||
        !clr_packing_problem_is_valid(problem)) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }
    if (problem->rule.piece_set_profile_id !=
        CLR_PIECE_SET_STANDARD_TETROMINOES) {
        return CLEARRA_PACKING_INVALID_PIECE;
    }

    uint32_t operation_table_version = CLEARRA_STANDARD_OPERATION_TABLE_VERSION;
    ClearraCacheIdentity identity = clearra_cache_identity_from_packing_problem(
        problem, operation_table_version);
    uint64_t batch_id = clearra_cache_identity_hash(identity);
    batch_id = clearra_cache_key_mix_u64(batch_id, problem->problem_kind);
    batch_id = clearra_cache_key_mix_u64(
        batch_id, problem->piece_source.piece_source_id);
    batch_id = clearra_cache_key_mix_u64(
        batch_id, problem->piece_source_pattern_id);

    *out_context = (clr_static_prune_context){0};
    out_context->batch_id = nonzero_identity(batch_id);
    out_context->operation_table_version = operation_table_version;
    out_context->piece_set_id = problem->rule.piece_set_profile_id;
    out_context->rule_profile_id = problem->rule.rule_profile_id;
    out_context->kick_profile_id = problem->rule.kick_profile_id;
    return clearra_packing_prune_context_is_valid(out_context)
               ? CLEARRA_PACKING_OK
               : CLEARRA_PACKING_INVALID_ARGUMENT;
}ClearraPackingStatus clearra_packing_prune_context_for_geometry(
    ClearraBoard64Layout layout,
    uint64_t occupied_board,
    uint64_t target_mask,
    clr_static_prune_context *out_context) {
    if (out_context == 0 || !clearra_board64_layout_is_valid(layout) ||
        ((occupied_board | target_mask) & ~layout.all_cells_mask) != 0u) {
        return CLEARRA_PACKING_INVALID_ARGUMENT;
    }

    uint64_t batch_id = UINT64_C(1469598103934665603);
    batch_id = clearra_cache_key_mix_u64(batch_id, layout.width);
    batch_id = clearra_cache_key_mix_u64(batch_id, layout.height);
    batch_id = clearra_cache_key_mix_u64(batch_id, occupied_board);
    batch_id = clearra_cache_key_mix_u64(batch_id, target_mask);

    *out_context = (clr_static_prune_context){0};
    out_context->batch_id = nonzero_identity(batch_id);
    out_context->operation_table_version =
        CLEARRA_STANDARD_OPERATION_TABLE_VERSION;
    out_context->piece_set_id = CLR_PIECE_SET_STANDARD_TETROMINOES;
    out_context->rule_profile_id = CLR_RULE_SRS_PLUS;
    out_context->kick_profile_id = CLR_KICK_SRS_PLUS_180;
    return CLEARRA_PACKING_OK;
}
