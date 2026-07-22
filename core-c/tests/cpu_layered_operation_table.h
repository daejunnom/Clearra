#ifndef CLEARRA_CPU_LAYERED_OPERATION_TABLE_H
#define CLEARRA_CPU_LAYERED_OPERATION_TABLE_H

#include "../src/gpu/gpu_backend.h"

typedef struct ClearraCpuLayeredOperationTable {
    ClearraPlacementCandidateList levels[CLR_STANDARD_PIECE_KIND_COUNT];
    uint8_t piece_counts[CLR_STANDARD_PIECE_KIND_COUNT];
    uint8_t piece_count;
} ClearraCpuLayeredOperationTable;
ClearraGpuStatus clearra_cpu_layered_operation_table_build(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraBoard64Layout layout,
    uint64_t active_region_mask,
    const clr_static_prune_context *static_prune_context,
    clr_pruning_proof_ledger *pruning_ledger,
    ClearraCpuLayeredOperationTable *out_view);
#endif
