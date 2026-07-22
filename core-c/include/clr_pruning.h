#ifndef CLR_PRUNING_H
#define CLR_PRUNING_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#define CLR_PRUNING_LEDGER_MAX_ENTRIES 64u
#define CLR_PRUNING_MINIMAL_RECORD_MAX_ENTRIES 32u
#define CLR_PRUNING_PRUNE_REASON_COUNT 26u
typedef enum clr_prune_reason {
    CLR_PRUNE_AREA_OVERFLOW = 1,
    CLR_PRUNE_PIECE_COUNT_OVERFLOW = 2,
    CLR_PRUNE_PLACEMENT_COLLISION = 3,
    CLR_PRUNE_TARGET_MASK_OVERFLOW = 4,
    CLR_PRUNE_ROW_CAPACITY_OVERFLOW = 5,
    CLR_PRUNE_CELL_DOMAIN_EMPTY_UNDER_CLEAR_STATE = 6,
    CLR_PRUNE_CELL_DOMAIN_EMPTY_FOR_ALL_REACHABLE_CLEAR_STATES = 7,
    CLR_PRUNE_FORCED_PIECE_FAMILY_UNDER_CLEAR_STATE = 8,
    CLR_PRUNE_FORCED_PIECE_FAMILY_FOR_ALL_REACHABLE_CLEAR_STATES = 9,
    CLR_PRUNE_COMPONENT_EXACT_COVER_IMPOSSIBLE = 10,
    CLR_PRUNE_HOLD_AUTOMATON_IMPOSSIBLE = 11,
    CLR_PRUNE_REACHABILITY_IMPOSSIBLE = 12,
    CLR_PRUNE_BUILD_ORDERS_HOLD_REACHABLE_INTERSECTION_EMPTY = 13,
    CLR_PRUNE_RESOURCE_BUDGET_EXCEEDED = 14,
    CLR_PRUNE_LINE_CLEAR_ORDER_IMPOSSIBLE = 15,
    CLR_PRUNE_COLUMN_DEMAND_OVERFLOW = 16,
    CLR_PRUNE_FULL_PARENT_DOMAIN_EMPTY = 17,
    CLR_PRUNE_SAME_TILE_PARENT_DOMAIN_EMPTY = 18,
    CLR_PRUNE_ADDITIVE_INVARIANT_MISMATCH = 19,
    CLR_PRUNE_SEPARATOR_COMPONENT_INFEASIBLE = 20,
    CLR_PRUNE_PARENT_DOMAIN_HALL_VIOLATION = 21,
    CLR_PRUNE_COLUMN_DEMAND_UNREACHABLE = 22,
    CLR_PRUNE_BUMPER_DOMAIN_EMPTY = 23,
    CLR_PRUNE_BUMPER_BRIDGE_INCOMPATIBLE = 24,
    CLR_PRUNE_REALIZATION_DOMAIN_EMPTY = 25
} clr_prune_reason;typedef enum clr_prune_proof_level {
    CLR_PRUNE_PROOF_LOCAL_ONLY = 1,
    CLR_PRUNE_PROOF_CLEAR_STATE_CONDITIONAL = 2,
    CLR_PRUNE_PROOF_ALL_REACHABLE_CLEAR_STATES = 3,
    CLR_PRUNE_PROOF_GLOBAL_SAFE = 4
} clr_prune_proof_level;typedef enum clr_prune_fallback_action {
    CLR_PRUNE_FALLBACK_KEEP_CANDIDATE = 1,
    CLR_PRUNE_FALLBACK_RUN_BUILDUP = 2,
    CLR_PRUNE_FALLBACK_DISABLE_DOMAIN_PRUNING = 3
} clr_prune_fallback_action;typedef enum clr_pruning_evidence_policy {
    CLR_PRUNING_EVIDENCE_BEST_EFFORT = 1,
    CLR_PRUNING_EVIDENCE_COMPLETE_REQUIRED = 2
} clr_pruning_evidence_policy;typedef enum clr_pruning_status {
    CLR_PRUNING_OK = 0,
    CLR_PRUNING_INVALID_ARGUMENT = 1,
    CLR_PRUNING_EVIDENCE_CAPACITY_UNAVAILABLE = 5
} clr_pruning_status;typedef enum clr_pruning_producer_id {
    CLR_PRUNING_PRODUCER_STATIC_PLACEMENT_FILTER = 1,
    CLR_PRUNING_PRODUCER_GEOMETRY_FULL_PLACEMENT_DOMAIN = 3,
    CLR_PRUNING_PRODUCER_GEOMETRY_ADDITIVE_INVARIANT = 4,
    CLR_PRUNING_PRODUCER_GEOMETRY_COMPONENT_DECOMPOSITION = 5,
    CLR_PRUNING_PRODUCER_BUILDUP_LINE_CLEAR_ORDER = 6,
    CLR_PRUNING_PRODUCER_GEOMETRY_COLUMN_PROJECTION = 7,
    CLR_PRUNING_PRODUCER_GEOMETRY_APDP = 8,
    CLR_PRUNING_PRODUCER_GEOMETRY_HALL_BOUND = 9,
    CLR_PRUNING_PRODUCER_GEOMETRY_BUMPER_DOMAIN = 10,
    CLR_PRUNING_PRODUCER_REALIZATION_FEASIBILITY = 11
} clr_pruning_producer_id;typedef uint64_t clr_pruning_batch_id;
typedef uint64_t clr_pruning_clear_state_key;
typedef uint64_t clr_pruning_component_key;
typedef uint64_t clr_pruning_board_profile_id;
typedef uint64_t clr_pruning_piece_set_id;
typedef uint64_t clr_pruning_evidence_digest;typedef struct clr_placement_domain_key {
    clr_pruning_component_key component_key;
    clr_pruning_clear_state_key clear_state_key;
    clr_pruning_board_profile_id board_profile_id;
    clr_pruning_piece_set_id piece_set_id;
} clr_placement_domain_key;typedef struct clr_placement_domain {
    clr_placement_domain_key key;
    uint32_t candidate_placement_count;
    uint64_t allowed_piece_mask;
    uint8_t forced_piece_family;
    uint8_t has_forced_piece_family;
    uint8_t proof_level;
    uint8_t reserved;
} clr_placement_domain;typedef struct clr_static_prune_context {
    clr_pruning_batch_id batch_id;
    uint8_t state_layer;
    uint8_t piece;
    uint8_t rotation;
    int8_t x;
    int8_t y;
    uint16_t operation_id;
    uint64_t operation_table_version;
    uint64_t piece_set_id;
    uint64_t rule_profile_id;
    uint64_t kick_profile_id;
} clr_static_prune_context;typedef struct clr_propagation_budget {
    uint64_t max_cpu_time_per_batch_ms;
    uint32_t max_components_per_batch;
    uint32_t max_component_cells;
    uint32_t max_candidate_domains;
    uint32_t max_clear_states;
    double min_expected_reduction_ratio;
} clr_propagation_budget;typedef struct clr_pruning_proof_ledger_entry {
    clr_pruning_batch_id batch_id;
    uint64_t producer_id;
    uint64_t catalog_identity_digest;
    uint8_t state_layer;
    uint8_t prune_reason;
    uint8_t proof_level;
    uint8_t fallback_if_invalid;
    uint32_t affected_candidate_count;
    uint8_t has_clear_state_key;
    uint8_t reserved[7];
    clr_pruning_clear_state_key clear_state_key;
    clr_pruning_evidence_digest evidence_digest;
} clr_pruning_proof_ledger_entry;typedef struct clr_pruning_minimal_record {
    clr_pruning_batch_id batch_id;
    uint64_t producer_id;
    uint64_t catalog_identity_digest;
    clr_pruning_evidence_digest aggregate_evidence_digest;
    uint64_t affected_candidate_count;
    uint8_t prune_reason;
    uint8_t reserved[7];
} clr_pruning_minimal_record;typedef struct clr_pruning_proof_ledger {
    uint16_t count;
    uint16_t capacity;
    uint16_t minimal_record_count;
    uint16_t minimal_record_capacity;
    uint8_t evidence_truncated;
    uint8_t evidence_policy;
    uint8_t complete_required_capacity_hit;
    uint8_t minimal_record_capacity_hit;
    uint32_t dropped_evidence_count;
    uint32_t candidates_kept_due_to_evidence_capacity;
    uint32_t prune_reason_counts[CLR_PRUNING_PRUNE_REASON_COUNT];
    clr_pruning_proof_ledger_entry entries[CLR_PRUNING_LEDGER_MAX_ENTRIES];
    clr_pruning_minimal_record
        minimal_records[CLR_PRUNING_MINIMAL_RECORD_MAX_ENTRIES];
} clr_pruning_proof_ledger;const char *clr_prune_reason_name(clr_prune_reason reason);
bool clr_prune_reason_is_forbidden_name(const char *name);
bool clr_prune_reason_has_connected_engine_factory(clr_prune_reason reason);
bool clr_prune_proof_level_allows_global_prune(clr_prune_proof_level level);void clr_placement_domain_init(
    clr_placement_domain *domain,
    clr_placement_domain_key key,
    uint32_t candidate_placement_count,
    uint64_t allowed_piece_mask,
    clr_prune_proof_level proof_level);
void clr_placement_domain_set_forced_piece_family(
    clr_placement_domain *domain,
    uint8_t piece_family,
    clr_prune_proof_level proof_level);
bool clr_cell_domain_empty_under_clear_state(const clr_placement_domain *domain);
bool clr_component_domain_has_forced_piece_family_under_clear_state(
    const clr_placement_domain *domain);
clr_pruning_evidence_digest clr_component_domain_digest_with_operation_table(
    const clr_placement_domain *domain,
    uint64_t operation_table_version);
clr_prune_proof_level clr_clear_state_domain_promote_if_all_reachable_clear_states(
    uint32_t proven_clear_state_count,
    uint32_t reachable_clear_state_count);
bool clr_component_exact_cover_runs_only_under_budget(
    const clr_propagation_budget *budget,
    uint32_t component_count,
    uint32_t component_cells,
    uint32_t candidate_domain_count,
    uint32_t clear_state_count);void clr_pruning_proof_ledger_init(clr_pruning_proof_ledger *ledger);
clr_pruning_status clr_pruning_proof_ledger_init_with_policy(
    clr_pruning_proof_ledger *ledger,
    clr_pruning_evidence_policy policy);
clr_pruning_status clr_pruning_proof_ledger_record(
    clr_pruning_proof_ledger *ledger,
    clr_pruning_proof_ledger_entry entry);
#endif
