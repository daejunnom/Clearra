#include "clr_coverage.h"
#include "clr_execution_control.h"
static uint32_t popcount_u64(uint64_t value) {
    uint32_t count = 0;
    while (value != 0) {
        count += (uint32_t)(value & UINT64_C(1));
        value >>= 1;
    }
    return count;
}uint16_t clr_pattern_bitset_word_count(uint32_t pattern_count) {
    if (pattern_count == 0 || pattern_count > CLR_COVERAGE_MAX_PATTERNS) {
        return 0;
    }
    return (uint16_t)((pattern_count + 63u) / 64u);
}clr_coverage_status clr_pattern_bitset_init(
    clr_pattern_bitset_c *bitset,
    uint32_t pattern_count) {
    return clr_pattern_bitset_init_with_identity(
        bitset,
        UINT64_C(0),
        UINT64_C(0),
        pattern_count);
}clr_coverage_status clr_pattern_bitset_init_with_identity(
    clr_pattern_bitset_c *bitset,
    uint64_t pattern_universe_id,
    uint64_t pattern_weight_model_id,
    uint32_t pattern_count) {
    if (bitset == 0 || pattern_count == 0) {
        return CLR_COVERAGE_INVALID_ARGUMENT;
    }

    uint16_t word_count = clr_pattern_bitset_word_count(pattern_count);
    if (word_count == 0 || word_count > CLR_COVERAGE_MAX_WORDS) {
        return CLR_COVERAGE_CAPACITY_EXCEEDED;
    }

    *bitset = (clr_pattern_bitset_c){0};
    bitset->pattern_universe_id = pattern_universe_id;
    bitset->pattern_weight_model_id = pattern_weight_model_id;
    bitset->pattern_count = pattern_count;
    bitset->word_count = word_count;
    return CLR_COVERAGE_OK;
}clr_coverage_status clr_pattern_bitset_insert(
    clr_pattern_bitset_c *bitset,
    uint32_t pattern_id) {
    if (bitset == 0 || bitset->word_count == 0 ||
        bitset->word_count > CLR_COVERAGE_MAX_WORDS) {
        return CLR_COVERAGE_INVALID_ARGUMENT;
    }
    if (pattern_id >= bitset->pattern_count) {
        return CLR_COVERAGE_PATTERN_OUT_OF_RANGE;
    }

    bitset->words[pattern_id / 64u] |= UINT64_C(1) << (pattern_id % 64u);
    return CLR_COVERAGE_OK;
}uint32_t clr_pattern_bitset_count_ones(const clr_pattern_bitset_c *bitset) {
    if (bitset == 0 || bitset->word_count > CLR_COVERAGE_MAX_WORDS) {
        return 0;
    }

    uint32_t count = 0;
    for (uint16_t index = 0; index < bitset->word_count; index++) {
        count += popcount_u64(bitset->words[index]);
    }
    return count;
}clr_coverage_status clr_pattern_bitset_union_checked(
    const clr_pattern_bitset_c *left,
    const clr_pattern_bitset_c *right,
    clr_pattern_bitset_c *out_union) {
    uint32_t cancellation_poll_counter = 0u;
    if (clr_execution_cancel_requested()) {
        return CLR_COVERAGE_CANCELLED;
    }
    if (left == 0 || right == 0 || out_union == 0) {
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

    *out_union = *left;
    for (uint16_t index = 0; index < left->word_count; index++) {
        if (clr_execution_control_poll(&cancellation_poll_counter)) {
            return CLR_COVERAGE_CANCELLED;
        }
        out_union->words[index] = left->words[index] | right->words[index];
    }
    return CLR_COVERAGE_OK;
}
