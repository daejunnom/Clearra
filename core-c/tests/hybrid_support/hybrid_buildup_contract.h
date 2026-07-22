#ifndef CLEARRA_HYBRID_BUILDUP_CONTRACT_H
#define CLEARRA_HYBRID_BUILDUP_CONTRACT_H

#include "hybrid_status.h"
#include "../../src/gpu/gpu_backend.h"
#include "../../include/clr_coverage.h"
typedef enum ClearraHybridBuildUpDispatchMode {
    CLEARRA_HYBRID_BUILDUP_VERIFY_FIRST = 0,
    CLEARRA_HYBRID_BUILDUP_ENUMERATE_VARIANTS = 1,
    CLEARRA_HYBRID_BUILDUP_COUNT_VARIANTS = 2
} ClearraHybridBuildUpDispatchMode;typedef struct ClearraHybridBuildVariantCollection {
    ClearraHybridBuildUpDispatchMode mode;
    uint16_t variant_count;
    uint64_t counted_variant_count;
    uint8_t count_complete;
    uint8_t trace_retained;
    uint8_t verify_first_used_for_coverage;
} ClearraHybridBuildVariantCollection;typedef struct ClearraHybridCoverageRowBridgeReport {
    uint16_t row_count;
    uint8_t from_enumerate_variants;
    uint8_t rejected_verify_first;
    uint64_t pattern_universe_id;
    uint64_t pattern_weight_model_id;
} ClearraHybridCoverageRowBridgeReport;ClearraHybridStatus clearra_hybrid_buildup_dispatch_candidate(
    const clr_packing_problem *packing,
    const ClearraPackingCandidateView *candidate,
    uint16_t coverage_pattern_id,
    ClearraHybridBuildUpDispatchMode mode,
    clr_build_variant_buffer *out_variants,
    clr_buildup_count_report *out_count_report);
ClearraHybridStatus clearra_hybrid_build_variant_buffer_append_checked(
    clr_build_variant_buffer *buffer,
    const clr_build_variant_view *variant);
ClearraHybridStatus clearra_hybrid_collect_build_variants_from_confirmed_queue(
    const clr_packing_problem *packing,
    const ClearraGpuConfirmedCandidateQueue *queue,
    ClearraHybridBuildUpDispatchMode mode,
    clr_build_variant_buffer *candidate_scratch,
    clr_build_variant_buffer *out_buffer,
    ClearraHybridBuildVariantCollection *out_collection);
ClearraHybridStatus clearra_hybrid_buildup_variants_from_confirmed_queue(
    const clr_packing_problem *packing,
    const ClearraGpuConfirmedCandidateQueue *queue,
    clr_build_variant_buffer *out_buffer);
ClearraHybridStatus clearra_hybrid_coverage_rows_from_build_variants(
    ClearraHybridBuildUpDispatchMode source_mode,
    const clr_build_variant_buffer *variants,
    uint64_t piece_source_id,
    uint64_t pattern_universe_id,
    uint64_t pattern_weight_model_id,
    uint32_t pattern_count,
    clr_coverage_row_view *out_rows,
    uint16_t row_capacity,
    ClearraHybridCoverageRowBridgeReport *out_report);
#endif
