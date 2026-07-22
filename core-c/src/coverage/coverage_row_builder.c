#include "clr_coverage.h"
#include "clr_execution_control.h"
static int coverage_row_kind_supported(uint32_t row_kind) {
    return row_kind <= (uint32_t)CLR_COVERAGE_ROW_KIND_SCORE_CELL;
}static int verification_source_can_source_build_variant_coverage(uint32_t source) {
    return source == (uint32_t)CLR_COVERAGE_VERIFICATION_PATTERN_SPECIFIC_BUILDUP;
}clr_coverage_status clr_coverage_row_from_verified_build_variant_with_identity(
    const clr_build_variant_view *variant,
    const clr_coverage_pattern_verification *verification,
    uint64_t piece_source_id,
    uint64_t pattern_universe_id,
    uint64_t pattern_weight_model_id,
    uint32_t pattern_count,
    clr_coverage_row_view *out_row) {
    if (clr_execution_cancel_requested()) {
        return CLR_COVERAGE_CANCELLED;
    }
    if (variant == 0 || verification == 0 || out_row == 0) {
        return CLR_COVERAGE_INVALID_ARGUMENT;
    }
    if (piece_source_id == UINT64_C(0) ||
        pattern_universe_id == UINT64_C(0) ||
        pattern_weight_model_id == UINT64_C(0)) {
        return CLR_COVERAGE_INVALID_ARGUMENT;
    }
    if (pattern_count == 0u) {
        return CLR_COVERAGE_INVALID_ARGUMENT;
    }
    if (variant->candidate_id == UINT64_C(0) ||
        variant->canonical_operation_set_id == UINT64_C(0)) {
        return CLR_COVERAGE_INVALID_ARGUMENT;
    }
    if (verification->accepted == 0u ||
        !verification_source_can_source_build_variant_coverage(
            verification->source)) {
        return CLR_COVERAGE_PATTERN_NOT_VERIFIED;
    }
    if (verification->pattern_id >= pattern_count) {
        return CLR_COVERAGE_PATTERN_OUT_OF_RANGE;
    }
    if (variant->coverage_pattern_id != verification->pattern_id) {
        return CLR_COVERAGE_PATTERN_NOT_VERIFIED;
    }

    *out_row = (clr_coverage_row_view){0};
    out_row->candidate_id = variant->candidate_id;
    out_row->piece_source_id = piece_source_id;
    out_row->row_kind = (uint32_t)CLR_COVERAGE_ROW_KIND_BUILD;
    out_row->coverage_pattern_id = verification->pattern_id;
    out_row->pattern_universe_id = pattern_universe_id;
    out_row->pattern_weight_model_id = pattern_weight_model_id;

    clr_coverage_status status =
        clr_pattern_bitset_init_with_identity(
            &out_row->patterns,
            out_row->pattern_universe_id,
            out_row->pattern_weight_model_id,
            pattern_count);
    if (status != CLR_COVERAGE_OK) {
        return status;
    }
    if (!coverage_row_kind_supported(out_row->row_kind)) {
        return CLR_COVERAGE_ROW_KIND_UNSUPPORTED;
    }
    return clr_pattern_bitset_insert(
        &out_row->patterns,
        verification->pattern_id);
}
