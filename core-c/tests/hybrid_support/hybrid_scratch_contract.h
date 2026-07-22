#ifndef CLEARRA_HYBRID_SCRATCH_CONTRACT_H
#define CLEARRA_HYBRID_SCRATCH_CONTRACT_H

#include "hybrid_buildup_contract.h"
#include "clr_memory.h"

typedef struct ClearraHybridScratch {
    ClearraCanonicalPackingTable *cpu_table;
    ClearraPackingCandidateBuffer *cpu_raw_candidates;
    clr_build_variant_buffer *candidate_variants;
    clr_build_variant_buffer *cpu_variants;
    clr_build_variant_buffer *hybrid_variants;
    clr_coverage_row_view *cpu_coverage_rows;
    clr_coverage_row_view *hybrid_coverage_rows;
    ClrScope *owner_scope;
} ClearraHybridScratch;
ClearraHybridStatus clearra_hybrid_scratch_create(
    ClrScope *owner_scope,
    ClearraHybridScratch *out_scratch);
#endif
