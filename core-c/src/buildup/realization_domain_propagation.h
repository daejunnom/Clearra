#ifndef CLEARRA_REALIZATION_DOMAIN_PROPAGATION_H
#define CLEARRA_REALIZATION_DOMAIN_PROPAGATION_H

#include "clr_buildup_status.h"
#include "../packing/geometry_catalog_internal.h"

typedef struct ClearraRealizationCandidateDomain {
    uint32_t skeleton_id;
    uint32_t realization_begin;
    uint32_t realization_count;
    uint32_t active_word_offset;
    uint32_t active_word_count;
    uint64_t compact_deleted_states;
    uint16_t contributing_rows;
    uint16_t required_deleted_rows;
    uint8_t compact;
    uint8_t reserved[3];
} ClearraRealizationCandidateDomain;

typedef enum ClearraRealizationDomainPropagationStatus {
    CLEARRA_REALIZATION_DOMAIN_SUPPORTED = 0,
    CLEARRA_REALIZATION_DOMAIN_INFEASIBLE = 1,
    CLEARRA_REALIZATION_DOMAIN_INVALID = 2
} ClearraRealizationDomainPropagationStatus;

typedef struct ClearraRealizationDomainPropagationInput {
    const ClearraGeometryCatalog *catalog;
    const ClearraRealizationCandidateDomain *domains;
    const uint16_t *contributors;
    const uint16_t *required_predecessors;
    uint64_t *active_realization_words;
    uint64_t *supported_realization_words;
    size_t realization_word_count;
    uint16_t clearable_rows;
    uint16_t terminal_state;
    uint8_t operation_count;
} ClearraRealizationDomainPropagationInput;

typedef struct ClearraRealizationDomainPropagationResult {
    uint32_t reachable_state_count;
    uint32_t live_state_count;
    uint32_t active_realization_count;
    uint32_t removed_realization_count;
    uint8_t complete;
    uint8_t reserved[3];
} ClearraRealizationDomainPropagationResult;

uint16_t clearra_realization_deleted_rows_for_state(
    const uint16_t contributors[16],
    uint16_t clearable_rows,
    uint16_t placed_operations);

bool clearra_realization_domain_supports_deleted_state(
    const ClearraGeometryCatalog *catalog,
    const ClearraRealizationCandidateDomain *domain,
    uint16_t deleted_rows);

bool clearra_realization_structural_transition_allowed(
    const ClearraRealizationDomainPropagationInput *input,
    uint16_t state,
    uint8_t operation);

bool clearra_realization_domain_value_is_active(
    const ClearraRealizationDomainPropagationInput *input,
    uint8_t operation,
    uint32_t local_realization_index);

bool clearra_realization_domain_common_predecessors(
    const ClearraRealizationDomainPropagationInput *input,
    uint16_t out_predecessors[CLR_BUILDUP_MAX_OPERATIONS]);

ClearraRealizationDomainPropagationStatus
clearra_realization_domain_propagate(
    const ClearraRealizationDomainPropagationInput *input,
    uint32_t *reachable_generations,
    uint32_t *live_generations,
    size_t state_capacity,
    uint32_t generation,
    ClearraRealizationDomainPropagationResult *out_result);

#endif
