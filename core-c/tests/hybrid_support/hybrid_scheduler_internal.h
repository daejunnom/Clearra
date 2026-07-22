#ifndef CLEARRA_HYBRID_SCHEDULER_INTERNAL_H
#define CLEARRA_HYBRID_SCHEDULER_INTERNAL_H

#include "hybrid_scheduler.h"

#include <time.h>
ClearraHybridStatus clearra_hybrid_status_from_packing(
    ClearraPackingStatus status);
ClearraHybridStatus clearra_hybrid_status_from_memory(ClrMemStatus status);
uint32_t clearra_hybrid_elapsed_ms_since(clock_t started);
ClearraHybridStatus clearra_hybrid_reduce_cpu_reference(
    const ClearraGpuPackingBatchDescriptor *batch,
    ClearraPackingCandidateBuffer *raw,
    ClearraCanonicalPackingTable *out_table);
#endif
