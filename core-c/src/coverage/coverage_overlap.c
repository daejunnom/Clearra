#include "clr_coverage.h"
#include "clr_execution_control.h"
static uint32_t popcount_u64(uint64_t value) {
    uint32_t count = 0;
    while (value != 0) {
        count += (uint32_t)(value & UINT64_C(1));
        value >>= 1;
    }
    return count;
}clr_coverage_status clr_coverage_overlap_count(
    const clr_pattern_bitset_c *left,
    const clr_pattern_bitset_c *right,
    clr_coverage_overlap_report_c *out_report) {
    uint32_t cancellation_poll_counter = 0u;
    if (clr_execution_cancel_requested()) {
        return CLR_COVERAGE_CANCELLED;
    }
    if (left == 0 || right == 0 || out_report == 0) {
        return CLR_COVERAGE_INVALID_ARGUMENT;
    }
    if (left->pattern_universe_id != right->pattern_universe_id ||
        left->pattern_count != right->pattern_count ||
        left->word_count != right->word_count) {
        return CLR_COVERAGE_PATTERN_UNIVERSE_MISMATCH;
    }
    if (left->pattern_weight_model_id != right->pattern_weight_model_id) {
        return CLR_COVERAGE_WEIGHT_MODEL_MISMATCH;
    }
    if (left->word_count == 0 || left->word_count > CLR_COVERAGE_MAX_WORDS) {
        return CLR_COVERAGE_INVALID_ARGUMENT;
    }

    *out_report = (clr_coverage_overlap_report_c){0};
    for (uint16_t index = 0; index < left->word_count; index++) {
        if (clr_execution_control_poll(&cancellation_poll_counter)) {
            return CLR_COVERAGE_CANCELLED;
        }
        out_report->overlap_count +=
            popcount_u64(left->words[index] & right->words[index]);
    }
    out_report->has_overlap = out_report->overlap_count > 0 ? 1u : 0u;
    return CLR_COVERAGE_OK;
}
