#ifndef CLEARRA_REALIZATION_FEASIBILITY_H
#define CLEARRA_REALIZATION_FEASIBILITY_H

#include "../packing/packing_problem.h"

typedef enum ClearraRealizationFeasibilityKind {
    CLEARRA_REALIZATION_FEASIBILITY_UNKNOWN = 0,
    CLEARRA_REALIZATION_FEASIBILITY_FEASIBLE = 1,
    CLEARRA_REALIZATION_FEASIBILITY_INFEASIBLE = 2
} ClearraRealizationFeasibilityKind;

typedef struct ClearraRealizationFeasibilityResult {
    uint64_t explored_state_count;
    uint64_t evidence_digest;
    uint16_t required_predecessors[CLR_BUILDUP_MAX_OPERATIONS];
    uint8_t operation_order[CLR_BUILDUP_MAX_OPERATIONS];
    uint8_t operation_count;
    uint8_t kind;
    uint8_t complete;
    uint8_t prune_authorized;
    uint8_t reserved[4];
} ClearraRealizationFeasibilityResult;

typedef struct ClearraRealizationFeasibilityWorkspace {
    uint32_t *state_generations;
    uint64_t *realization_words;
    size_t state_capacity;
    size_t realization_word_capacity;
    uint32_t generation;
    uint8_t generation_plane_count;
    uint8_t reserved[3];
} ClearraRealizationFeasibilityWorkspace;

ClearraPackingStatus clearra_realization_feasibility_analyze(
    const ClearraGeometryCatalog *catalog,
    const clr_packing_problem *problem,
    const uint32_t *skeleton_row_ids,
    uint8_t operation_count,
    ClearraRealizationFeasibilityWorkspace *workspace,
    clr_pruning_proof_ledger *pruning_ledger,
    ClearraRealizationFeasibilityResult *out_result);

void clearra_realization_feasibility_workspace_release(
    ClearraRealizationFeasibilityWorkspace *workspace);

size_t clearra_realization_feasibility_workspace_retained_bytes(
    const ClearraRealizationFeasibilityWorkspace *workspace);

#endif
