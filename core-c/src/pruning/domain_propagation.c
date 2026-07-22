#include "clr_pruning.h"

static uint64_t mix_u64(uint64_t hash, uint64_t value) {
    hash ^= value;
    hash *= UINT64_C(1099511628211);
    return hash;
}

void clr_placement_domain_init(
    clr_placement_domain *domain,
    clr_placement_domain_key key,
    uint32_t candidate_placement_count,
    uint64_t allowed_piece_mask,
    clr_prune_proof_level proof_level) {
    if (domain == 0) {
        return;
    }
    *domain = (clr_placement_domain){0};
    domain->key = key;
    domain->candidate_placement_count = candidate_placement_count;
    domain->allowed_piece_mask = allowed_piece_mask;
    domain->proof_level = (uint8_t)proof_level;
}

bool clr_cell_domain_empty_under_clear_state(const clr_placement_domain *domain) {
    return domain != 0 && domain->candidate_placement_count == 0u &&
           domain->proof_level == CLR_PRUNE_PROOF_CLEAR_STATE_CONDITIONAL;
}

void clr_placement_domain_set_forced_piece_family(
    clr_placement_domain *domain,
    uint8_t piece_family,
    clr_prune_proof_level proof_level) {
    if (domain == 0) {
        return;
    }
    domain->forced_piece_family = piece_family;
    domain->has_forced_piece_family = 1u;
    domain->proof_level = (uint8_t)proof_level;
}

bool clr_component_domain_has_forced_piece_family_under_clear_state(
    const clr_placement_domain *domain) {
    return domain != 0 && domain->has_forced_piece_family != 0u &&
           domain->proof_level == CLR_PRUNE_PROOF_CLEAR_STATE_CONDITIONAL;
}

clr_pruning_evidence_digest clr_component_domain_digest_with_operation_table(
    const clr_placement_domain *domain,
    uint64_t operation_table_version) {
    if (domain == 0 || operation_table_version == 0u) {
        return 0u;
    }
    uint64_t hash = UINT64_C(1469598103934665603);
    hash = mix_u64(hash, domain->key.component_key);
    hash = mix_u64(hash, domain->key.clear_state_key);
    hash = mix_u64(hash, domain->key.board_profile_id);
    hash = mix_u64(hash, domain->key.piece_set_id);
    hash = mix_u64(hash, domain->candidate_placement_count);
    hash = mix_u64(hash, domain->allowed_piece_mask);
    hash = mix_u64(hash, domain->forced_piece_family);
    hash = mix_u64(hash, domain->has_forced_piece_family);
    hash = mix_u64(hash, domain->proof_level);
    hash = mix_u64(hash, operation_table_version);
    return hash == 0u ? UINT64_C(1) : hash;
}

clr_prune_proof_level
clr_clear_state_domain_promote_if_all_reachable_clear_states(
    uint32_t proven_clear_state_count,
    uint32_t reachable_clear_state_count) {
    if (reachable_clear_state_count == 0u ||
        proven_clear_state_count < reachable_clear_state_count) {
        return CLR_PRUNE_PROOF_CLEAR_STATE_CONDITIONAL;
    }
    return CLR_PRUNE_PROOF_ALL_REACHABLE_CLEAR_STATES;
}

bool clr_component_exact_cover_runs_only_under_budget(
    const clr_propagation_budget *budget,
    uint32_t component_count,
    uint32_t component_cells,
    uint32_t candidate_domain_count,
    uint32_t clear_state_count) {
    if (budget == 0) {
        return false;
    }
    return component_count <= budget->max_components_per_batch &&
           component_cells <= budget->max_component_cells &&
           candidate_domain_count <= budget->max_candidate_domains &&
           clear_state_count <= budget->max_clear_states;
}
