
#include "clr_coverage.h"

#include <stdio.h>
#include <stdlib.h>

#define EXPECT_COVERAGE_STATUS(EXPR, EXPECTED)                                            \
    do {                                                                                  \
        clr_coverage_status actual_status = (EXPR);                                       \
        if (actual_status != (EXPECTED)) {                                                \
            fprintf(stderr, "%s:%d expected coverage status %d but got %d\n", __FILE__,   \
                    __LINE__, (int)(EXPECTED), (int)actual_status);                       \
            exit(1);                                                                      \
        }                                                                                 \
    } while (0)

#define EXPECT_U64(EXPR, EXPECTED)                                                        \
    do {                                                                                  \
        uint64_t actual_value = (uint64_t)(EXPR);                                         \
        uint64_t expected_value = (uint64_t)(EXPECTED);                                   \
        if (actual_value != expected_value) {                                             \
            fprintf(stderr, "%s:%d expected 0x%llx but got 0x%llx\n", __FILE__,           \
                    __LINE__, (unsigned long long)expected_value,                         \
                    (unsigned long long)actual_value);                                    \
            exit(1);                                                                      \
        }                                                                                 \
    } while (0)

#define EXPECT_TRUE(EXPR)                                                                 \
    do {                                                                                  \
        if (!(EXPR)) {                                                                    \
            fprintf(stderr, "%s:%d expected true\n", __FILE__, __LINE__);                \
            exit(1);                                                                      \
        }                                                                                 \
    } while (0)

#define EXPECT_FALSE(EXPR)                                                                \
    do {                                                                                  \
        if ((EXPR)) {                                                                     \
            fprintf(stderr, "%s:%d expected false\n", __FILE__, __LINE__);               \
            exit(1);                                                                      \
        }                                                                                 \
    } while (0)
static clr_build_variant_view build_variant(uint64_t candidate_id, uint32_t pattern_id) {
    clr_build_variant_view variant = {0};
    variant.candidate_id = candidate_id;
    variant.canonical_operation_set_id = candidate_id;
    variant.operation_set_hash = candidate_id;
    variant.coverage_pattern_id = pattern_id;
    variant.placed_count = 5;
    variant.queue_cursor = 5;
    return variant;
}static clr_coverage_pattern_verification verified_pattern(
    uint32_t pattern_id,
    uint32_t source,
    uint8_t accepted) {
    clr_coverage_pattern_verification verification = {0};
    verification.pattern_id = pattern_id;
    verification.source = source;
    verification.accepted = accepted;
    return verification;
}static clr_coverage_status build_coverage_row(
    const clr_build_variant_view *variant,
    uint32_t pattern_count,
    clr_coverage_row_view *out_row) {
    clr_coverage_pattern_verification verification = verified_pattern(
        variant->coverage_pattern_id,
        (uint32_t)CLR_COVERAGE_VERIFICATION_PATTERN_SPECIFIC_BUILDUP,
        1u);
    return clr_coverage_row_from_verified_build_variant_with_identity(
        variant,
        &verification,
        UINT64_C(11),
        UINT64_C(7),
        UINT64_C(9),
        pattern_count,
        out_row);
}static void coverage_row_builder_uses_stable_candidate_id(void) {
    clr_build_variant_view variant = build_variant(UINT64_C(0x12345678), 3);
    clr_coverage_row_view row;

    EXPECT_COVERAGE_STATUS(build_coverage_row(&variant, 8, &row), CLR_COVERAGE_OK);
    EXPECT_U64(row.candidate_id, UINT64_C(0x12345678));
    EXPECT_U64(row.piece_source_id, 11);
    EXPECT_U64(row.row_kind, CLR_COVERAGE_ROW_KIND_BUILD);
    EXPECT_U64(row.coverage_pattern_id, 3);
    EXPECT_U64(row.pattern_universe_id, 7);
    EXPECT_U64(row.pattern_weight_model_id, 9);
    EXPECT_U64(row.patterns.pattern_universe_id, 7);
    EXPECT_U64(row.patterns.pattern_weight_model_id, 9);
    EXPECT_U64(row.patterns.pattern_count, 8);
    EXPECT_U64(row.patterns.word_count, 1);
    EXPECT_U64(row.patterns.words[0], UINT64_C(1) << 3);
}static void coverage_row_builder_requires_identity(void) {
    clr_build_variant_view variant = build_variant(UINT64_C(0x12345678), 3);
    clr_coverage_row_view row;

    EXPECT_COVERAGE_STATUS(
        clr_coverage_row_from_verified_build_variant_with_identity(
            &variant,
            &(clr_coverage_pattern_verification){
                .pattern_id = 3u,
                .source =
                    (uint32_t)CLR_COVERAGE_VERIFICATION_PATTERN_SPECIFIC_BUILDUP,
                .accepted = 1u,
            },
            UINT64_C(0),
            UINT64_C(7),
            UINT64_C(9),
            8,
            &row),
        CLR_COVERAGE_INVALID_ARGUMENT);
    EXPECT_COVERAGE_STATUS(
        clr_coverage_row_from_verified_build_variant_with_identity(
            &variant,
            &(clr_coverage_pattern_verification){
                .pattern_id = 3u,
                .source =
                    (uint32_t)CLR_COVERAGE_VERIFICATION_PATTERN_SPECIFIC_BUILDUP,
                .accepted = 1u,
            },
            UINT64_C(11),
            UINT64_C(0),
            UINT64_C(9),
            8,
            &row),
        CLR_COVERAGE_INVALID_ARGUMENT);
    EXPECT_COVERAGE_STATUS(
        clr_coverage_row_from_verified_build_variant_with_identity(
            &variant,
            &(clr_coverage_pattern_verification){
                .pattern_id = 3u,
                .source =
                    (uint32_t)CLR_COVERAGE_VERIFICATION_PATTERN_SPECIFIC_BUILDUP,
                .accepted = 1u,
            },
            UINT64_C(11),
            UINT64_C(7),
            UINT64_C(0),
            8,
            &row),
        CLR_COVERAGE_INVALID_ARGUMENT);
    EXPECT_COVERAGE_STATUS(
        clr_coverage_row_from_verified_build_variant_with_identity(
            &variant,
            &(clr_coverage_pattern_verification){
                .pattern_id = 3u,
                .source =
                    (uint32_t)CLR_COVERAGE_VERIFICATION_PATTERN_SPECIFIC_BUILDUP,
                .accepted = 1u,
            },
            UINT64_C(11),
            UINT64_C(7),
            UINT64_C(9),
            0,
            &row),
        CLR_COVERAGE_INVALID_ARGUMENT);
}static void product_coverage_row_rejects_zero_piece_source_id(void) {
    clr_build_variant_view variant = build_variant(UINT64_C(0x12345678), 3);
    clr_coverage_row_view row;

    EXPECT_COVERAGE_STATUS(
        clr_coverage_row_from_verified_build_variant_with_identity(
            &variant,
            &(clr_coverage_pattern_verification){
                .pattern_id = 3u,
                .source =
                    (uint32_t)CLR_COVERAGE_VERIFICATION_PATTERN_SPECIFIC_BUILDUP,
                .accepted = 1u,
            },
            UINT64_C(0),
            UINT64_C(7),
            UINT64_C(9),
            8,
            &row),
        CLR_COVERAGE_INVALID_ARGUMENT);
}static void coverage_row_builder_allows_zero_identity_only_in_test_helper(void) {
    clr_build_variant_view variant = build_variant(UINT64_C(0x12345678), 3);
    clr_coverage_row_view row;

    EXPECT_COVERAGE_STATUS(
        clr_coverage_test_row_from_build_variant_without_identity(&variant, 8, &row),
        CLR_COVERAGE_OK);
    EXPECT_U64(row.pattern_universe_id, 0);
    EXPECT_U64(row.pattern_weight_model_id, 0);
    EXPECT_U64(row.patterns.pattern_universe_id, 0);
    EXPECT_U64(row.patterns.pattern_weight_model_id, 0);
}static void test_helper_identityless_row_not_exported_in_public_product_path(void) {
    clr_build_variant_view variant = build_variant(UINT64_C(0x12345678), 3);
    clr_coverage_row_view rows[1];
    clr_pattern_bitset_c covered;

    EXPECT_COVERAGE_STATUS(
        clr_coverage_test_row_from_build_variant_without_identity(&variant, 8, &rows[0]),
        CLR_COVERAGE_OK);
    EXPECT_COVERAGE_STATUS(
        clr_coverage_union_rows(rows, 1, &covered),
        CLR_COVERAGE_INVALID_ARGUMENT);
}static void pattern_bitset_universe_mismatch_is_error(void) {
    clr_pattern_bitset_c left;
    clr_pattern_bitset_c right;
    clr_pattern_bitset_c output;

    EXPECT_COVERAGE_STATUS(
        clr_pattern_bitset_init_with_identity(&left, UINT64_C(7), UINT64_C(9), 8),
        CLR_COVERAGE_OK);
    EXPECT_COVERAGE_STATUS(
        clr_pattern_bitset_init_with_identity(&right, UINT64_C(8), UINT64_C(9), 8),
        CLR_COVERAGE_OK);

    EXPECT_COVERAGE_STATUS(
        clr_pattern_bitset_union_checked(&left, &right, &output),
        CLR_COVERAGE_PATTERN_UNIVERSE_MISMATCH);
}static void pattern_weight_model_mismatch_is_error(void) {
    clr_pattern_bitset_c left;
    clr_pattern_bitset_c right;
    clr_pattern_bitset_c output;

    EXPECT_COVERAGE_STATUS(
        clr_pattern_bitset_init_with_identity(&left, UINT64_C(7), UINT64_C(1), 8),
        CLR_COVERAGE_OK);
    EXPECT_COVERAGE_STATUS(
        clr_pattern_bitset_init_with_identity(&right, UINT64_C(7), UINT64_C(2), 8),
        CLR_COVERAGE_OK);

    EXPECT_COVERAGE_STATUS(
        clr_pattern_bitset_union_checked(&left, &right, &output),
        CLR_COVERAGE_WEIGHT_MODEL_MISMATCH);
}static void c_coverage_capacity_statuses_are_distinct_contracts(void) {
    EXPECT_U64(CLR_COVERAGE_CAPACITY_EXCEEDED, 6);
    EXPECT_U64(CLR_SCORE_MATRIX_CAPACITY_EXCEEDED, 7);
    EXPECT_U64(CLR_SPIN_COVERAGE_CAPACITY_EXCEEDED, 8);
    EXPECT_U64(CLR_COVERAGE_PIECE_SOURCE_MISMATCH, 9);
    EXPECT_U64(CLR_COVERAGE_PATTERN_NOT_VERIFIED, 10);
}static void coverage_row_union_uses_or_semantics(void) {
    clr_build_variant_view first = build_variant(UINT64_C(0x11), 1);
    clr_build_variant_view second = build_variant(UINT64_C(0x22), 6);
    clr_coverage_row_view rows[2];
    clr_pattern_bitset_c covered;

    EXPECT_COVERAGE_STATUS(build_coverage_row(&first, 8, &rows[0]), CLR_COVERAGE_OK);
    EXPECT_COVERAGE_STATUS(build_coverage_row(&second, 8, &rows[1]), CLR_COVERAGE_OK);
    EXPECT_COVERAGE_STATUS(clr_coverage_union_rows(rows, 2, &covered), CLR_COVERAGE_OK);

    EXPECT_U64(covered.words[0], (UINT64_C(1) << 1) | (UINT64_C(1) << 6));
    EXPECT_U64(clr_pattern_bitset_count_ones(&covered), 2);
}static void coverage_row_union_rejects_unsupported_row_kind(void) {
    clr_build_variant_view first = build_variant(UINT64_C(0x11), 1);
    clr_coverage_row_view rows[1];
    clr_pattern_bitset_c covered;

    EXPECT_COVERAGE_STATUS(build_coverage_row(&first, 8, &rows[0]), CLR_COVERAGE_OK);
    rows[0].row_kind = 99u;

    EXPECT_COVERAGE_STATUS(
        clr_coverage_union_rows(rows, 1, &covered),
        CLR_COVERAGE_ROW_KIND_UNSUPPORTED);
}static void coverage_row_rejects_piece_source_mismatch(void) {
    clr_build_variant_view first = build_variant(UINT64_C(0x11), 1);
    clr_build_variant_view second = build_variant(UINT64_C(0x22), 2);
    clr_coverage_row_view rows[2];
    clr_pattern_bitset_c covered;

    EXPECT_COVERAGE_STATUS(build_coverage_row(&first, 8, &rows[0]), CLR_COVERAGE_OK);
    EXPECT_COVERAGE_STATUS(build_coverage_row(&second, 8, &rows[1]), CLR_COVERAGE_OK);
    rows[1].piece_source_id = UINT64_C(12);

    EXPECT_COVERAGE_STATUS(
        clr_coverage_union_rows(rows, 2, &covered),
        CLR_COVERAGE_PIECE_SOURCE_MISMATCH);
}static void coverage_union_rejects_piece_source_mismatch(void) {
    coverage_row_rejects_piece_source_mismatch();
}static void same_pattern_universe_different_piece_source_not_or_merged(void) {
    clr_build_variant_view first = build_variant(UINT64_C(0x11), 1);
    clr_build_variant_view second = build_variant(UINT64_C(0x22), 6);
    clr_coverage_row_view rows[2];
    clr_pattern_bitset_c covered;

    EXPECT_COVERAGE_STATUS(build_coverage_row(&first, 8, &rows[0]), CLR_COVERAGE_OK);
    EXPECT_COVERAGE_STATUS(build_coverage_row(&second, 8, &rows[1]), CLR_COVERAGE_OK);
    rows[1].piece_source_id = UINT64_C(12);

    EXPECT_U64(rows[0].pattern_universe_id, rows[1].pattern_universe_id);
    EXPECT_U64(rows[0].pattern_weight_model_id, rows[1].pattern_weight_model_id);
    EXPECT_COVERAGE_STATUS(
        clr_coverage_union_rows(rows, 2, &covered),
        CLR_COVERAGE_PIECE_SOURCE_MISMATCH);
}static void coverage_overlap_reports_duplicate_patterns(void) {
    clr_build_variant_view first = build_variant(UINT64_C(0x11), 1);
    clr_build_variant_view second = build_variant(UINT64_C(0x22), 1);
    clr_coverage_row_view rows[2];
    clr_coverage_overlap_report_c report;

    EXPECT_COVERAGE_STATUS(build_coverage_row(&first, 8, &rows[0]), CLR_COVERAGE_OK);
    EXPECT_COVERAGE_STATUS(build_coverage_row(&second, 8, &rows[1]), CLR_COVERAGE_OK);
    EXPECT_COVERAGE_STATUS(
        clr_coverage_overlap_count(&rows[0].patterns, &rows[1].patterns, &report),
        CLR_COVERAGE_OK);

    EXPECT_U64(report.has_overlap, 1);
    EXPECT_U64(report.overlap_count, 1);
}static void coverage_row_requires_pattern_specific_validation(void) {
    clr_build_variant_view variant = build_variant(UINT64_C(0x12345678), 3);
    clr_coverage_row_view row;
    clr_coverage_pattern_verification verification = verified_pattern(
        3u,
        (uint32_t)CLR_COVERAGE_VERIFICATION_PATTERN_SPECIFIC_BUILDUP,
        1u);

    EXPECT_COVERAGE_STATUS(
        clr_coverage_row_from_verified_build_variant_with_identity(
            &variant,
            &verification,
            UINT64_C(11),
            UINT64_C(7),
            UINT64_C(9),
            8u,
            &row),
        CLR_COVERAGE_OK);

    verification.accepted = 0u;
    EXPECT_COVERAGE_STATUS(
        clr_coverage_row_from_verified_build_variant_with_identity(
            &variant,
            &verification,
            UINT64_C(11),
            UINT64_C(7),
            UINT64_C(9),
            8u,
            &row),
        CLR_COVERAGE_PATTERN_NOT_VERIFIED);

    verification = verified_pattern(
        4u,
        (uint32_t)CLR_COVERAGE_VERIFICATION_PATTERN_SPECIFIC_BUILDUP,
        1u);
    EXPECT_COVERAGE_STATUS(
        clr_coverage_row_from_verified_build_variant_with_identity(
            &variant,
            &verification,
            UINT64_C(11),
            UINT64_C(7),
            UINT64_C(9),
            8u,
            &row),
        CLR_COVERAGE_PATTERN_NOT_VERIFIED);
}static void verify_first_cannot_source_coverage(void) {
    clr_build_variant_view variant = build_variant(UINT64_C(0x12345678), 3);
    clr_coverage_row_view row;
    clr_coverage_pattern_verification verification = verified_pattern(
        3u,
        (uint32_t)CLR_COVERAGE_VERIFICATION_VERIFY_FIRST,
        1u);

    EXPECT_COVERAGE_STATUS(
        clr_coverage_row_from_verified_build_variant_with_identity(
            &variant,
            &verification,
            UINT64_C(11),
            UINT64_C(7),
            UINT64_C(9),
            8u,
            &row),
        CLR_COVERAGE_PATTERN_NOT_VERIFIED);
}static void coverage_pattern_id_injection_without_pattern_verification_rejected(void) {
    clr_build_variant_view injected_variant =
        build_variant(UINT64_C(0x3333), 6u);
    clr_coverage_pattern_verification verification = verified_pattern(
        2u,
        (uint32_t)CLR_COVERAGE_VERIFICATION_PATTERN_SPECIFIC_BUILDUP,
        1u);
    clr_coverage_row_view row;

    EXPECT_COVERAGE_STATUS(
        clr_coverage_row_from_verified_build_variant_with_identity(
            &injected_variant,
            &verification,
            UINT64_C(42),
            UINT64_C(7),
            UINT64_C(9),
            8u,
            &row),
        CLR_COVERAGE_PATTERN_NOT_VERIFIED);
}
int main(void) {
    coverage_row_builder_uses_stable_candidate_id();
    coverage_row_builder_requires_identity();
    product_coverage_row_rejects_zero_piece_source_id();
    coverage_row_builder_allows_zero_identity_only_in_test_helper();
    test_helper_identityless_row_not_exported_in_public_product_path();
    pattern_bitset_universe_mismatch_is_error();
    pattern_weight_model_mismatch_is_error();
    c_coverage_capacity_statuses_are_distinct_contracts();
    coverage_row_union_uses_or_semantics();
    coverage_row_union_rejects_unsupported_row_kind();
    coverage_row_rejects_piece_source_mismatch();
    coverage_union_rejects_piece_source_mismatch();
    same_pattern_universe_different_piece_source_not_or_merged();
    coverage_overlap_reports_duplicate_patterns();
    coverage_row_requires_pattern_specific_validation();
    verify_first_cannot_source_coverage();
    coverage_pattern_id_injection_without_pattern_verification_rejected();
    puts("core-c coverage tests passed");
    return 0;
}


