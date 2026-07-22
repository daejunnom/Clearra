#include "cache_identity.h"

#define CLEARRA_STANDARD_PIECE_DEFINITION_ID_FINGERPRINT UINT64_C(0x5354445049434544)
#define CLEARRA_STANDARD_PIECE_AREA_MULTISET_FINGERPRINT UINT64_C(0x5354444152454134)
ClearraCacheIdentity clearra_cache_identity_zero(void) {
    ClearraCacheIdentity identity;
    identity.board = 0;
    identity.piece_set_profile = 0;
    identity.piece_definition_id_fingerprint = 0;
    identity.piece_area_multiset_fingerprint = 0;
    identity.rule_kick_profile = 0;
    identity.backend_mode = 0;
    identity.operation_table_version = 0;
    identity.supply_provenance = 0;
    identity.queue_pattern_id = 0;
    identity.piece_window_start = 0;
    identity.piece_window_len = 0;
    identity.goal_id = 0;
    return identity;
}bool clearra_cache_identity_is_complete(ClearraCacheIdentity identity) {
    bool has_supply_identity =
        identity.supply_provenance != 0 || identity.queue_pattern_id != 0;
    return identity.piece_set_profile != 0 &&
           identity.piece_definition_id_fingerprint != 0 &&
           identity.piece_area_multiset_fingerprint != 0 &&
           identity.rule_kick_profile != 0 &&
           identity.backend_mode != 0 &&
           identity.operation_table_version != 0 &&
           has_supply_identity &&
           identity.piece_window_len != 0 &&
           identity.goal_id != 0;
}static uint64_t board_identity_from_descriptor(const clr_board_descriptor *board) {
    uint64_t hash = UINT64_C(1469598103934665603);
    hash = clearra_cache_key_mix_u64(hash, board->width);
    hash = clearra_cache_key_mix_u64(hash, board->visible_height);
    hash = clearra_cache_key_mix_u64(hash, board->search_height);
    hash = clearra_cache_key_mix_u64(hash, board->initial_mask);
    hash = clearra_cache_key_mix_u64(hash, board->initial_mask_hi);
    hash = clearra_cache_key_mix_u64(hash, board->backend_kind);
    hash = clearra_cache_key_mix_u64(hash, board->cell_count);
    return hash;
}static uint64_t rule_kick_identity_from_descriptor(
    const clr_rule_profile_descriptor *rule) {
    uint64_t hash = UINT64_C(1469598103934665603);
    hash = clearra_cache_key_mix_u64(hash, rule->rule_profile_id);
    hash = clearra_cache_key_mix_u64(hash, rule->kick_profile_id);
    hash = clearra_cache_key_mix_u64(hash, rule->spawn_profile_id);
    hash = clearra_cache_key_mix_u64(hash, rule->has_verified_kick_profile);
    hash = clearra_cache_key_mix_u64(hash, rule->verified_supports_180);
    hash = clearra_cache_key_mix_u64(hash, rule->verified_transition_count);
    for (uint16_t index = 0; index < rule->verified_transition_count &&
                              index < CLR_RULE_MAX_KICK_TRANSITIONS;
         index++) {
        const clr_kick_transition_descriptor *transition =
            &rule->verified_transitions[index];
        hash = clearra_cache_key_mix_u64(hash, transition->piece);
        hash = clearra_cache_key_mix_u64(hash, transition->from_rotation);
        hash = clearra_cache_key_mix_u64(hash, transition->to_rotation);
        hash = clearra_cache_key_mix_u64(hash, transition->sequence.count);
        for (uint8_t offset_index = 0;
             offset_index < transition->sequence.count &&
             offset_index < CLR_RULE_MAX_KICK_OFFSETS;
             offset_index++) {
            const clr_kick_offset_descriptor *offset =
                &transition->sequence.offsets[offset_index];
            hash = clearra_cache_key_mix_u64(hash, (uint8_t)offset->dx);
            hash = clearra_cache_key_mix_u64(hash, (uint8_t)offset->dy);
        }
    }
    return hash;
}static uint32_t piece_source_pattern_id_from_problem(
    const clr_packing_problem *problem) {
    uint64_t hash = UINT64_C(1469598103934665603);
    const clr_piece_source_descriptor *source = &problem->piece_source;
    const clr_piece_multiset_window *window = &problem->piece_multiset_window;
    hash = clearra_cache_key_mix_u64(hash, source->source_kind);
    hash = clearra_cache_key_mix_u64(hash, source->piece_source_id);
    hash = clearra_cache_key_mix_u64(hash, source->provenance_id);
    hash = clearra_cache_key_mix_u64(hash, source->pattern_universe_id);
    hash = clearra_cache_key_mix_u64(hash, source->pattern_weight_model_id);
    hash = clearra_cache_key_mix_u64(hash, source->materialized_pattern_count);
    hash = clearra_cache_key_mix_u64(hash, source->fixed_sequence_len);
    hash = clearra_cache_key_mix_u64(hash, source->complete);
    hash = clearra_cache_key_mix_u64(hash, source->truncation_reason);
    hash = clearra_cache_key_mix_u64(hash, window->total_count);
    hash = clearra_cache_key_mix_u64(hash, window->exact_count);
    for (uint8_t piece = CLR_PIECE_I; piece <= CLR_PIECE_L; piece++) {
        hash = clearra_cache_key_mix_u64(hash, window->counts[piece]);
    }
    hash = clearra_cache_key_mix_u64(
        hash,
        clearra_piece_multiset_family_digest(&problem->piece_multiset_family));
    uint32_t id = (uint32_t)(hash & UINT32_C(0xffffffff));
    return id == 0u ? 1u : id;
}static uint64_t goal_identity_from_problem(const clr_packing_problem *problem) {
    uint64_t hash = UINT64_C(1469598103934665603);
    hash = clearra_cache_key_mix_u64(hash, problem->goal);
    hash = clearra_cache_key_mix_u64(hash, problem->count_policy);
    hash = clearra_cache_key_mix_u64(hash, problem->objective);
    hash = clearra_cache_key_mix_u64(hash, problem->piece_window.has_exact_pieces);
    hash = clearra_cache_key_mix_u64(hash, problem->piece_window.exact_pieces);
    hash = clearra_cache_key_mix_u64(hash, problem->goal_region_mask);
    hash = clearra_cache_key_mix_u64(hash, problem->required_fill_mask);
    hash = clearra_cache_key_mix_u64(hash, problem->forbidden_mask);
    return hash;
}ClearraCacheIdentity clearra_cache_identity_from_packing_problem(
    const clr_packing_problem *problem,
    uint32_t operation_table_version) {
    ClearraCacheIdentity identity = clearra_cache_identity_zero();
    if (problem == 0) {
        return identity;
    }
    identity.board = board_identity_from_descriptor(&problem->board);
    identity.piece_set_profile = problem->rule.piece_set_profile_id;
    identity.piece_definition_id_fingerprint =
        CLEARRA_STANDARD_PIECE_DEFINITION_ID_FINGERPRINT;
    identity.piece_area_multiset_fingerprint =
        CLEARRA_STANDARD_PIECE_AREA_MULTISET_FINGERPRINT;
    identity.rule_kick_profile =
        rule_kick_identity_from_descriptor(&problem->rule);
    identity.backend_mode = problem->backend.requested_backend;
    identity.operation_table_version = operation_table_version;
    identity.supply_provenance = problem->piece_source.provenance_id;
    identity.queue_pattern_id = piece_source_pattern_id_from_problem(problem);
    identity.piece_window_start =
        problem->piece_window.has_exact_pieces ? 0u : problem->piece_window.max_pieces;
    identity.piece_window_len = problem->piece_window.max_pieces;
    identity.goal_id = goal_identity_from_problem(problem);
    return identity;
}
