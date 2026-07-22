#include "clr_coverage.h"
#include "clr_execution_control.h"
static int coverage_row_kind_supported(uint32_t row_kind) {
    return row_kind <= (uint32_t)CLR_COVERAGE_ROW_KIND_SCORE_CELL;
}clr_coverage_status clr_coverage_union_rows(
    const clr_coverage_row_view *rows,
    uint16_t row_count,
    clr_pattern_bitset_c *out_union) {
    uint32_t cancellation_poll_counter = 0u;
    if (clr_execution_cancel_requested()) {
        return CLR_COVERAGE_CANCELLED;
    }
    if (rows == 0 || row_count == 0 || out_union == 0) {
        return CLR_COVERAGE_INVALID_ARGUMENT;
    }
    if (!coverage_row_kind_supported(rows[0].row_kind)) {
        return CLR_COVERAGE_ROW_KIND_UNSUPPORTED;
    }
    if (rows[0].piece_source_id == UINT64_C(0)) {
        return CLR_COVERAGE_INVALID_ARGUMENT;
    }

    clr_coverage_status status =
        clr_pattern_bitset_init_with_identity(
            out_union,
            rows[0].pattern_universe_id,
            rows[0].pattern_weight_model_id,
            rows[0].patterns.pattern_count);
    if (status != CLR_COVERAGE_OK) {
        return status;
    }

    for (uint16_t index = 0; index < row_count; index++) {
        if (clr_execution_control_poll(&cancellation_poll_counter)) {
            return CLR_COVERAGE_CANCELLED;
        }
        if (!coverage_row_kind_supported(rows[index].row_kind)) {
            return CLR_COVERAGE_ROW_KIND_UNSUPPORTED;
        }
        if (rows[index].row_kind != rows[0].row_kind) {
            return CLR_COVERAGE_ROW_KIND_UNSUPPORTED;
        }
        if (rows[index].piece_source_id == UINT64_C(0)) {
            return CLR_COVERAGE_INVALID_ARGUMENT;
        }
        if (rows[index].pattern_universe_id != rows[0].pattern_universe_id) {
            return CLR_COVERAGE_PATTERN_UNIVERSE_MISMATCH;
        }
        if (rows[index].pattern_weight_model_id != rows[0].pattern_weight_model_id) {
            return CLR_COVERAGE_WEIGHT_MODEL_MISMATCH;
        }
        if (rows[index].piece_source_id != rows[0].piece_source_id) {
            return CLR_COVERAGE_PIECE_SOURCE_MISMATCH;
        }
        clr_pattern_bitset_c next_union;
        status = clr_pattern_bitset_union_checked(
            out_union,
            &rows[index].patterns,
            &next_union);
        if (status != CLR_COVERAGE_OK) {
            return status;
        }
        *out_union = next_union;
    }
    return CLR_COVERAGE_OK;
}
