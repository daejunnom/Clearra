#ifndef CLR_COVERAGE_H
#define CLR_COVERAGE_H

#include "clr_problem.h"

#include <stdint.h>

#define CLR_COVERAGE_MAX_PATTERNS 1024u
#define CLR_COVERAGE_MAX_WORDS 16u
typedef enum clr_coverage_status {
    CLR_COVERAGE_OK = 0,
    CLR_COVERAGE_INVALID_ARGUMENT = 1,
    CLR_COVERAGE_PATTERN_OUT_OF_RANGE = 2,
    CLR_COVERAGE_PATTERN_UNIVERSE_MISMATCH = 3,
    CLR_COVERAGE_WEIGHT_MODEL_MISMATCH = 4,
    CLR_COVERAGE_ROW_KIND_UNSUPPORTED = 5,
    CLR_COVERAGE_CAPACITY_EXCEEDED = 6,
    CLR_SCORE_MATRIX_CAPACITY_EXCEEDED = 7,
    CLR_SPIN_COVERAGE_CAPACITY_EXCEEDED = 8,
    CLR_COVERAGE_PIECE_SOURCE_MISMATCH = 9,
    CLR_COVERAGE_PATTERN_NOT_VERIFIED = 10,
    CLR_COVERAGE_CANCELLED = 11
} clr_coverage_status;typedef enum clr_coverage_row_kind {
    CLR_COVERAGE_ROW_KIND_PC = 0,
    CLR_COVERAGE_ROW_KIND_SETUP = 1,
    CLR_COVERAGE_ROW_KIND_BUILD = 2,
    CLR_COVERAGE_ROW_KIND_SPIN_TARGET = 3,
    CLR_COVERAGE_ROW_KIND_SCORE_CELL = 4
} clr_coverage_row_kind;typedef struct clr_pattern_bitset_c {
    uint64_t pattern_universe_id;
    uint64_t pattern_weight_model_id;
    uint32_t pattern_count;
    uint16_t word_count;
    uint16_t reserved;
    uint64_t words[CLR_COVERAGE_MAX_WORDS];
} clr_pattern_bitset_c;typedef struct clr_coverage_row_view {
    uint64_t candidate_id;
    uint64_t piece_source_id;
    uint32_t row_kind;
    uint32_t coverage_pattern_id;
    uint64_t pattern_universe_id;
    uint64_t pattern_weight_model_id;
    clr_pattern_bitset_c patterns;
} clr_coverage_row_view;typedef enum clr_coverage_verification_source {
    CLR_COVERAGE_VERIFICATION_PATTERN_SPECIFIC_BUILDUP = 1,
    CLR_COVERAGE_VERIFICATION_VERIFY_FIRST = 2
} clr_coverage_verification_source;typedef struct clr_coverage_pattern_verification {
    uint32_t pattern_id;
    uint32_t source;
    uint8_t accepted;
    uint8_t reserved[3];
} clr_coverage_pattern_verification;typedef struct clr_coverage_overlap_report_c {
    uint32_t overlap_count;
    uint8_t has_overlap;
    uint8_t reserved[3];
} clr_coverage_overlap_report_c;uint16_t clr_pattern_bitset_word_count(uint32_t pattern_count);
clr_coverage_status clr_pattern_bitset_init(
    clr_pattern_bitset_c *bitset,
    uint32_t pattern_count);
clr_coverage_status clr_pattern_bitset_init_with_identity(
    clr_pattern_bitset_c *bitset,
    uint64_t pattern_universe_id,
    uint64_t pattern_weight_model_id,
    uint32_t pattern_count);
clr_coverage_status clr_pattern_bitset_insert(
    clr_pattern_bitset_c *bitset,
    uint32_t pattern_id);
uint32_t clr_pattern_bitset_count_ones(const clr_pattern_bitset_c *bitset);
clr_coverage_status clr_pattern_bitset_union_checked(
    const clr_pattern_bitset_c *left,
    const clr_pattern_bitset_c *right,
    clr_pattern_bitset_c *out_union);
clr_coverage_status clr_coverage_row_from_verified_build_variant_with_identity(
    const clr_build_variant_view *variant,
    const clr_coverage_pattern_verification *verification,
    uint64_t piece_source_id,
    uint64_t pattern_universe_id,
    uint64_t pattern_weight_model_id,
    uint32_t pattern_count,
    clr_coverage_row_view *out_row);
#ifdef CLEARRA_CORE_TEST
static inline clr_coverage_status clr_coverage_test_row_from_build_variant_without_identity(
    const clr_build_variant_view *variant,
    uint32_t pattern_count,
    clr_coverage_row_view *out_row) {
    if (variant == 0 || out_row == 0 || pattern_count == 0) {
        return CLR_COVERAGE_INVALID_ARGUMENT;
    }
    if (variant->coverage_pattern_id >= pattern_count) {
        return CLR_COVERAGE_PATTERN_OUT_OF_RANGE;
    }

    *out_row = (clr_coverage_row_view){0};
    out_row->candidate_id = variant->candidate_id;
    out_row->row_kind = (uint32_t)CLR_COVERAGE_ROW_KIND_BUILD;
    out_row->coverage_pattern_id = variant->coverage_pattern_id;
    out_row->patterns.pattern_count = pattern_count;
    out_row->patterns.word_count = clr_pattern_bitset_word_count(pattern_count);
    if (out_row->patterns.word_count == 0 ||
        out_row->patterns.word_count > CLR_COVERAGE_MAX_WORDS) {
        return CLR_COVERAGE_CAPACITY_EXCEEDED;
    }
    return clr_pattern_bitset_insert(
        &out_row->patterns,
        variant->coverage_pattern_id);
}
#endif
clr_coverage_status clr_coverage_union_rows(
    const clr_coverage_row_view *rows,
    uint16_t row_count,
    clr_pattern_bitset_c *out_union);
clr_coverage_status clr_coverage_overlap_count(
    const clr_pattern_bitset_c *left,
    const clr_pattern_bitset_c *right,
    clr_coverage_overlap_report_c *out_report);
#endif
