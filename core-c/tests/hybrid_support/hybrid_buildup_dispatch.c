#include "hybrid_scheduler.h"
static ClearraHybridStatus packing_status_to_hybrid(ClearraPackingStatus status) {
    return status == CLEARRA_PACKING_OK ? CLEARRA_HYBRID_OK
                                        : CLEARRA_HYBRID_PACKING_ERROR;
}static ClearraHybridStatus buildup_status_to_hybrid(clr_buildup_status status) {
    return status == CLR_BUILDUP_OK ? CLEARRA_HYBRID_OK
                                    : CLEARRA_HYBRID_BUILDUP_ERROR;
}static uint8_t buildup_status_is_logical_reject(clr_buildup_status status) {
    return (uint8_t)(status >= CLR_BUILDUP_LINE_CLEAR_DEPENDENCY_IMPOSSIBLE &&
                     status <= CLR_BUILDUP_COLLISION);
}static uint8_t buildup_status_is_incomplete(clr_buildup_status status) {
    return (uint8_t)(status == CLR_BUILDUP_CAPACITY_EXCEEDED ||
                     status == CLR_KICK_EVIDENCE_BUFFER_EXHAUSTED ||
                     status == CLR_BUILDUP_ENUMERATION_TRUNCATED);
}ClearraHybridStatus clearra_hybrid_buildup_dispatch_candidate(
    const clr_packing_problem *packing,
    const ClearraPackingCandidateView *candidate,
    uint16_t coverage_pattern_id,
    ClearraHybridBuildUpDispatchMode mode,
    clr_build_variant_buffer *out_variants,
    clr_buildup_count_report *out_count_report) {
    if (packing == 0 || candidate == 0) {
        return CLEARRA_HYBRID_INVALID_ARGUMENT;
    }
    if (mode != CLEARRA_HYBRID_BUILDUP_COUNT_VARIANTS && out_variants == 0) {
        return CLEARRA_HYBRID_INVALID_ARGUMENT;
    }
    if (mode == CLEARRA_HYBRID_BUILDUP_COUNT_VARIANTS && out_count_report == 0) {
        return CLEARRA_HYBRID_INVALID_ARGUMENT;
    }

    clr_buildup_problem problem;
    ClearraPackingStatus packing_status =
        clearra_buildup_problem_from_packing_candidate(
            packing, candidate, coverage_pattern_id, &problem);
    if (packing_status != CLEARRA_PACKING_OK) {
        return packing_status_to_hybrid(packing_status);
    }

    if (mode == CLEARRA_HYBRID_BUILDUP_VERIFY_FIRST) {
        return buildup_status_to_hybrid(
            clr_buildup_verify_first(&problem, out_variants));
    }
    if (mode == CLEARRA_HYBRID_BUILDUP_ENUMERATE_VARIANTS) {
        clr_buildup_enumeration_limits limits = {
            .max_variants = 0u,
            .preserve_hold_branches = 1u,
        };
        clr_buildup_status status =
            clr_buildup_enumerate_variants(&problem, &limits, out_variants);
        if (out_count_report != 0) {
            uint8_t search_complete =
                (uint8_t)(status == CLR_BUILDUP_OK ||
                          buildup_status_is_logical_reject(status));
            *out_count_report = (clr_buildup_count_report){
                .total_variant_count = out_variants->count,
                .search_complete = search_complete,
                .solution_exists = (uint8_t)(out_variants->count > 0u),
                .count_complete = search_complete,
                .trace_retained = (uint8_t)(out_variants->count > 0u),
                .no_variant_reason =
                    buildup_status_is_logical_reject(status)
                        ? (uint32_t)status
                        : CLR_BUILDUP_OK,
                .truncation_reason =
                    buildup_status_is_incomplete(status)
                        ? (uint32_t)status
                        : CLR_BUILDUP_OK,
            };
        }
        if (status == CLR_BUILDUP_CAPACITY_EXCEEDED ||
            status == CLR_BUILDUP_ENUMERATION_TRUNCATED) {
            return CLEARRA_HYBRID_OK;
        }
        return buildup_status_to_hybrid(status);
    }
    if (mode == CLEARRA_HYBRID_BUILDUP_COUNT_VARIANTS) {
        clr_buildup_count_limits limits = {
            .max_variants = 0u,
            .preserve_hold_branches = 1u,
            .retain_traces = 0u,
        };
        return buildup_status_to_hybrid(
            clr_buildup_count_variants(&problem, &limits, out_count_report));
    }

    return CLEARRA_HYBRID_INVALID_ARGUMENT;
}

#include "hybrid_scheduler.h"
static ClearraHybridStatus gpu_status_to_hybrid(ClearraGpuStatus status) {
    return status == CLEARRA_GPU_OK ? CLEARRA_HYBRID_OK
                                    : CLEARRA_HYBRID_PACKING_ERROR;
}ClearraHybridStatus clearra_hybrid_build_variant_buffer_append_checked(
    clr_build_variant_buffer *buffer,
    const clr_build_variant_view *variant) {
    if (buffer == 0 || variant == 0) {
        return CLEARRA_HYBRID_INVALID_ARGUMENT;
    }
    if (buffer->count >= CLR_BUILDUP_MAX_VARIANTS) {
        return CLEARRA_HYBRID_BUILDUP_ERROR;
    }
    if (variant->kick_evidence_count > CLR_BUILDUP_MAX_KICK_EVIDENCE_PER_VARIANT) {
        return CLEARRA_HYBRID_BUILDUP_ERROR;
    }
    if (variant->kick_evidence_count > 0u && variant->kick_evidence == 0) {
        return CLEARRA_HYBRID_INVALID_ARGUMENT;
    }

    uint16_t destination = buffer->count;
    for (uint32_t index = 0; index < variant->kick_evidence_count; index++) {
        buffer->kick_evidence_storage[destination][index] =
            variant->kick_evidence[index];
    }
    buffer->variants[destination] = *variant;
    if (variant->kick_evidence_count > 0u) {
        buffer->variants[destination].kick_evidence =
            buffer->kick_evidence_storage[destination];
    } else {
        buffer->variants[destination].kick_evidence = 0;
    }
    buffer->count++;
    return CLEARRA_HYBRID_OK;
}static ClearraHybridStatus append_buffer(
    clr_build_variant_buffer *destination,
    const clr_build_variant_buffer *source,
    uint8_t *out_truncated) {
    if (destination == 0 || source == 0 || out_truncated == 0) {
        return CLEARRA_HYBRID_INVALID_ARGUMENT;
    }
    *out_truncated = 0u;
    for (uint16_t index = 0; index < source->count; index++) {
        if (destination->count >= CLR_BUILDUP_MAX_VARIANTS) {
            *out_truncated = 1u;
            return CLEARRA_HYBRID_OK;
        }
        ClearraHybridStatus status =
            clearra_hybrid_build_variant_buffer_append_checked(
                destination,
                &source->variants[index]);
        if (status != CLEARRA_HYBRID_OK) {
            return status;
        }
    }
    return CLEARRA_HYBRID_OK;
}ClearraHybridStatus clearra_hybrid_collect_build_variants_from_confirmed_queue(
    const clr_packing_problem *packing,
    const ClearraGpuConfirmedCandidateQueue *queue,
    ClearraHybridBuildUpDispatchMode mode,
    clr_build_variant_buffer *candidate_scratch,
    clr_build_variant_buffer *out_buffer,
    ClearraHybridBuildVariantCollection *out_collection) {
    if (packing == 0 || queue == 0 || out_collection == 0 ||
        queue->cpu_exact_confirmed == 0u ||
        queue->can_enter_cpu_buildup_queue == 0u ||
        queue->can_create_coverage_row != 0u ||
        queue->candidate_is_solution != 0u) {
        return CLEARRA_HYBRID_INVALID_ARGUMENT;
    }
    if (mode != CLEARRA_HYBRID_BUILDUP_COUNT_VARIANTS &&
        (candidate_scratch == 0 || out_buffer == 0)) {
        return CLEARRA_HYBRID_INVALID_ARGUMENT;
    }
    if (packing->piece_source_pattern_id > UINT16_MAX) {
        return CLEARRA_HYBRID_INVALID_ARGUMENT;
    }

    *out_collection = (ClearraHybridBuildVariantCollection){
        .mode = mode,
        .count_complete = 1u,
    };
    if (out_buffer != 0) {
        clr_build_variant_buffer_clear(out_buffer);
    }

    for (uint16_t index = 0; index < queue->count; index++) {
        ClearraPackingCandidateView candidate;
        ClearraGpuStatus gpu_status =
            clearra_gpu_confirmed_candidate_queue_candidate_at(
                queue, index, &candidate);
        if (gpu_status != CLEARRA_GPU_OK) {
            return gpu_status_to_hybrid(gpu_status);
        }

        clr_buildup_count_report count_report = {0};
        ClearraHybridStatus status =
            clearra_hybrid_buildup_dispatch_candidate(
                packing,
                &candidate,
                (uint16_t)packing->piece_source_pattern_id,
                mode,
                mode == CLEARRA_HYBRID_BUILDUP_COUNT_VARIANTS
                    ? 0
                    : candidate_scratch,
                &count_report);
        if (status != CLEARRA_HYBRID_OK) {
            return status;
        }

        if (mode == CLEARRA_HYBRID_BUILDUP_COUNT_VARIANTS) {
            out_collection->counted_variant_count +=
                count_report.total_variant_count;
            out_collection->count_complete =
                (uint8_t)(out_collection->count_complete &&
                          count_report.search_complete);
            out_collection->trace_retained |= count_report.trace_retained;
            continue;
        }

        out_collection->count_complete =
            (uint8_t)(out_collection->count_complete &&
                      count_report.search_complete);
        uint8_t append_truncated = 0u;
        status = append_buffer(
            out_buffer, candidate_scratch, &append_truncated);
        if (status != CLEARRA_HYBRID_OK) {
            return status;
        }
        if (append_truncated) {
            out_collection->count_complete = 0u;
            break;
        }
    }

    if (mode == CLEARRA_HYBRID_BUILDUP_COUNT_VARIANTS) {
        out_collection->variant_count = 0u;
        return CLEARRA_HYBRID_OK;
    }

    out_collection->variant_count = out_buffer->count;
    out_collection->counted_variant_count = out_buffer->count;
    out_collection->trace_retained = 1u;
    out_collection->verify_first_used_for_coverage =
        (uint8_t)(mode == CLEARRA_HYBRID_BUILDUP_VERIFY_FIRST);
    return CLEARRA_HYBRID_OK;
}

#include "hybrid_scheduler.h"
static ClearraHybridStatus coverage_status_to_hybrid(
    clr_coverage_status status) {
    return status == CLR_COVERAGE_OK ? CLEARRA_HYBRID_OK
                                     : CLEARRA_HYBRID_BUILDUP_ERROR;
}ClearraHybridStatus clearra_hybrid_coverage_rows_from_build_variants(
    ClearraHybridBuildUpDispatchMode source_mode,
    const clr_build_variant_buffer *variants,
    uint64_t piece_source_id,
    uint64_t pattern_universe_id,
    uint64_t pattern_weight_model_id,
    uint32_t pattern_count,
    clr_coverage_row_view *out_rows,
    uint16_t row_capacity,
    ClearraHybridCoverageRowBridgeReport *out_report) {
    if (out_report == 0) {
        return CLEARRA_HYBRID_INVALID_ARGUMENT;
    }
    *out_report = (ClearraHybridCoverageRowBridgeReport){
        .pattern_universe_id = pattern_universe_id,
        .pattern_weight_model_id = pattern_weight_model_id,
    };
    if (source_mode == CLEARRA_HYBRID_BUILDUP_VERIFY_FIRST) {
        out_report->rejected_verify_first = 1u;
        return CLEARRA_HYBRID_INVALID_ARGUMENT;
    }
    if (source_mode != CLEARRA_HYBRID_BUILDUP_ENUMERATE_VARIANTS ||
        variants == 0 || out_rows == 0 || piece_source_id == 0u ||
        pattern_count == 0u) {
        return CLEARRA_HYBRID_INVALID_ARGUMENT;
    }
    if (variants->count > row_capacity) {
        return CLEARRA_HYBRID_BUILDUP_ERROR;
    }

    for (uint16_t index = 0; index < variants->count; index++) {
        clr_coverage_pattern_verification verification = {
            .pattern_id = variants->variants[index].coverage_pattern_id,
            .source =
                (uint32_t)CLR_COVERAGE_VERIFICATION_PATTERN_SPECIFIC_BUILDUP,
            .accepted = 1u,
        };
        clr_coverage_status status =
            clr_coverage_row_from_verified_build_variant_with_identity(
                &variants->variants[index],
                &verification,
                piece_source_id,
                pattern_universe_id,
                pattern_weight_model_id,
                pattern_count,
                &out_rows[index]);
        if (status != CLR_COVERAGE_OK) {
            return coverage_status_to_hybrid(status);
        }
    }

    out_report->row_count = variants->count;
    out_report->from_enumerate_variants = 1u;
    return CLEARRA_HYBRID_OK;
}
