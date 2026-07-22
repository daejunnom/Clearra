#include "buildup_tests_support.h"
void buildup_modes_split_verify_first_enumerate_and_count(void) {
    ClearraBoard64Layout layout = buildup_test_standard_10x2_layout();
    uint8_t columns[5] = {0, 2, 4, 6, 8};
    clr_packing_problem packing = buildup_test_buildup_packing_problem(2, 5, 0);
    const uint8_t pieces[5] = {
        CLR_PIECE_O,
        CLR_PIECE_O,
        CLR_PIECE_O,
        CLR_PIECE_O,
        CLR_PIECE_O,
    };
    buildup_test_set_packing_pieces(
        &packing,
        pieces,
        5,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);

    clr_buildup_problem problem =
        buildup_test_build_problem_from_candidate(packing, buildup_test_o_candidate_for_columns(layout, columns, 5));
    clr_build_variant_buffer *first =
        (clr_build_variant_buffer *)malloc(sizeof(clr_build_variant_buffer));
    clr_build_variant_buffer *variants =
        (clr_build_variant_buffer *)malloc(sizeof(clr_build_variant_buffer));
    clr_buildup_enumeration_limits enumerate_limits = {0};
    clr_buildup_count_limits count_limits = {0};
    clr_buildup_count_report count_report;

    EXPECT_TRUE(first != 0);
    EXPECT_TRUE(variants != 0);

    enumerate_limits.max_variants = CLR_BUILDUP_MAX_VARIANTS;
    enumerate_limits.preserve_hold_branches = 1u;
    count_limits.max_variants = CLR_BUILDUP_MAX_VARIANTS;
    count_limits.preserve_hold_branches = 1u;
    count_limits.retain_traces = 0u;

    EXPECT_BUILDUP_STATUS(clr_buildup_verify_first(&problem, first),
                          CLR_BUILDUP_OK);
    EXPECT_BUILDUP_STATUS(clr_buildup_enumerate_variants(
                              &problem, &enumerate_limits, variants),
                          CLR_BUILDUP_OK);
    EXPECT_BUILDUP_STATUS(clr_buildup_count_variants(
                              &problem, &count_limits, &count_report),
                          CLR_BUILDUP_OK);

    EXPECT_U64(first->count, 1);
    EXPECT_U64(variants->count, 120);
    EXPECT_U64(count_report.total_variant_count, 120);
    EXPECT_U64(count_report.count_complete, 1);
    EXPECT_U64(count_report.trace_retained, 0);
    free(first);
    free(variants);
}void buildup_verify_first_returns_single_witness(void) {
    clr_buildup_problem problem = buildup_test_two_operation_gap_fill_problem();
    clr_build_variant_buffer *first =
        (clr_build_variant_buffer *)malloc(sizeof(clr_build_variant_buffer));

    EXPECT_TRUE(first != 0);

    EXPECT_BUILDUP_STATUS(clr_buildup_verify_first(&problem, first),
                          CLR_BUILDUP_OK);

    EXPECT_U64(first->count, 1);
    EXPECT_U64(first->variants[0].final_board, 0);
    EXPECT_U64(first->variants[0].placed_count, 2);

    free(first);
}void buildup_enumerate_variants_returns_expected_count_for_two_operation_fixture(void) {
    clr_buildup_problem problem = buildup_test_two_operation_gap_fill_problem();
    clr_build_variant_buffer *variants =
        (clr_build_variant_buffer *)malloc(sizeof(clr_build_variant_buffer));
    clr_buildup_enumeration_limits limits = {0};

    EXPECT_TRUE(variants != 0);
    limits.max_variants = CLR_BUILDUP_MAX_VARIANTS;

    EXPECT_BUILDUP_STATUS(clr_buildup_enumerate_variants(&problem, &limits, variants),
                          CLR_BUILDUP_OK);

    EXPECT_U64(variants->count, 2);
    EXPECT_U64(variants->variants[0].final_board, 0);
    EXPECT_U64(variants->variants[1].final_board, 0);
    EXPECT_U64(variants->variants[0].placed_count, 2);
    EXPECT_U64(variants->variants[1].placed_count, 2);
    EXPECT_U64(variants->variants[0].queue_cursor, 2);
    EXPECT_U64(variants->variants[1].queue_cursor, 2);
    EXPECT_U64(variants->variants[0].cleared_lines, 2);
    EXPECT_U64(variants->variants[1].cleared_lines, 2);

    free(variants);
}void buildup_count_variants_matches_enumerate_variants_for_small_fixture(void) {
    clr_buildup_problem problem = buildup_test_two_operation_gap_fill_problem();
    clr_build_variant_buffer *variants =
        (clr_build_variant_buffer *)malloc(sizeof(clr_build_variant_buffer));
    clr_buildup_enumeration_limits enumerate_limits = {0};
    clr_buildup_count_limits count_limits = {0};
    clr_buildup_count_report report;

    EXPECT_TRUE(variants != 0);
    enumerate_limits.max_variants = CLR_BUILDUP_MAX_VARIANTS;
    count_limits.max_variants = CLR_BUILDUP_MAX_VARIANTS;

    EXPECT_BUILDUP_STATUS(clr_buildup_enumerate_variants(
                              &problem, &enumerate_limits, variants),
                          CLR_BUILDUP_OK);
    EXPECT_BUILDUP_STATUS(clr_buildup_count_variants(
                              &problem, &count_limits, &report),
                          CLR_BUILDUP_OK);

    EXPECT_U64(variants->count, 2);
    EXPECT_U64(report.total_variant_count, variants->count);
    EXPECT_U64(report.count_complete, 1);
    EXPECT_U64(report.trace_retained, 0);

    free(variants);
}void buildup_count_variants_reports_total_count_without_retaining_traces(void) {
    clr_buildup_problem problem = buildup_test_two_operation_gap_fill_problem();
    clr_buildup_count_limits limits = {0};
    clr_buildup_count_report report;

    limits.max_variants = CLR_BUILDUP_MAX_VARIANTS;
    limits.retain_traces = 0u;

    EXPECT_BUILDUP_STATUS(clr_buildup_count_variants(&problem, &limits, &report),
                          CLR_BUILDUP_OK);

    EXPECT_U64(report.total_variant_count, 2);
    EXPECT_U64(report.search_complete, 1);
    EXPECT_U64(report.solution_exists, 1);
    EXPECT_U64(report.count_complete, 1);
    EXPECT_U64(report.trace_retained, 0);
    EXPECT_U64(report.retained_variant_count, 0);
    EXPECT_U64(report.no_variant_reason, CLR_BUILDUP_OK);
    EXPECT_U64(report.truncation_reason, CLR_BUILDUP_OK);
}void count_variants_zero_solution_is_complete(void) {
    ClearraBoard64Layout layout = buildup_test_standard_10x2_layout();
    uint8_t columns[1] = {0};
    clr_packing_problem packing = buildup_test_buildup_packing_problem(2, 1, 0);
    const uint8_t pieces[2] = {CLR_PIECE_I, CLR_PIECE_T};
    buildup_test_set_packing_pieces(
        &packing,
        pieces,
        2,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);
    clr_buildup_problem problem = buildup_test_build_problem_from_candidate(
        packing, buildup_test_o_candidate_for_columns(layout, columns, 1));
    clr_buildup_count_limits limits = {0};
    clr_buildup_count_report report;

    EXPECT_BUILDUP_STATUS(
        clr_buildup_count_variants(&problem, &limits, &report),
        CLR_BUILDUP_OK);
    EXPECT_U64(report.search_complete, 1);
    EXPECT_U64(report.count_complete, 1);
    EXPECT_U64(report.total_variant_count, 0);
    EXPECT_U64(report.solution_exists, 0);
    EXPECT_U64(report.no_variant_reason, CLR_BUILDUP_HOLD_DISABLED_IMPOSSIBLE);
    EXPECT_U64(report.truncation_reason, CLR_BUILDUP_OK);
}void count_variants_zero_max_variants_uses_default_budget(void) {
    clr_buildup_problem problem = buildup_test_two_operation_gap_fill_problem();
    clr_buildup_count_limits limits = {0};
    clr_buildup_count_report report;

    limits.max_variants = 0;

    EXPECT_BUILDUP_STATUS(clr_buildup_count_variants(&problem, &limits, &report),
                          CLR_BUILDUP_OK);

    EXPECT_U64(report.total_variant_count, 2);
    EXPECT_U64(report.count_complete, 1);
    EXPECT_U64(report.retained_variant_count, 0);
    EXPECT_U64(report.trace_retained, 0);
    EXPECT_U64(report.truncation_reason, CLR_BUILDUP_OK);
}void enumerate_and_count_zero_max_variants_have_same_default_semantics(void) {
    clr_buildup_problem problem = buildup_test_two_operation_gap_fill_problem();
    clr_build_variant_buffer *variants =
        (clr_build_variant_buffer *)malloc(sizeof(clr_build_variant_buffer));
    clr_buildup_enumeration_limits enumerate_limits = {0};
    clr_buildup_count_limits count_limits = {0};
    clr_buildup_count_report report;

    EXPECT_TRUE(variants != 0);
    enumerate_limits.max_variants = 0;
    count_limits.max_variants = 0;

    EXPECT_BUILDUP_STATUS(clr_buildup_enumerate_variants(
                              &problem, &enumerate_limits, variants),
                          CLR_BUILDUP_OK);
    EXPECT_BUILDUP_STATUS(clr_buildup_count_variants(
                              &problem, &count_limits, &report),
                          CLR_BUILDUP_OK);

    EXPECT_U64(variants->count, 2);
    EXPECT_U64(report.total_variant_count, variants->count);
    EXPECT_U64(report.count_complete, 1);
    EXPECT_U64(report.truncation_reason, CLR_BUILDUP_OK);

    free(variants);
}void enumerate_variants_truncates_after_limit_without_losing_prefix(void) {
    ClearraBoard64Layout layout = buildup_test_standard_10x2_layout();
    uint8_t columns[5] = {0, 2, 4, 6, 8};
    clr_packing_problem packing = buildup_test_buildup_packing_problem(2, 5, 0);
    const uint8_t pieces[5] = {
        CLR_PIECE_O,
        CLR_PIECE_O,
        CLR_PIECE_O,
        CLR_PIECE_O,
        CLR_PIECE_O,
    };
    buildup_test_set_packing_pieces(
        &packing,
        pieces,
        5,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);

    clr_buildup_problem problem =
        buildup_test_build_problem_from_candidate(packing, buildup_test_o_candidate_for_columns(layout, columns, 5));
    clr_build_variant_buffer *variants =
        (clr_build_variant_buffer *)malloc(sizeof(clr_build_variant_buffer));
    clr_buildup_enumeration_limits enumerate_limits = {0};
    clr_buildup_count_limits count_limits = {0};
    clr_buildup_count_report count_report;

    EXPECT_TRUE(variants != 0);

    enumerate_limits.max_variants = 1;
    count_limits.max_variants = 1;

    EXPECT_BUILDUP_STATUS(clr_buildup_enumerate_variants(
                              &problem, &enumerate_limits, variants),
                          CLR_BUILDUP_OK);
    EXPECT_U64(variants->count, 1);
    EXPECT_U64(variants->total_variant_count, 120);
    EXPECT_U64(variants->count_complete, 1);
    EXPECT_U64(variants->trace_retention_truncated, 1);
    EXPECT_BUILDUP_STATUS(clr_buildup_count_variants(
                              &problem, &count_limits, &count_report),
                          CLR_BUILDUP_ENUMERATION_TRUNCATED);
    EXPECT_U64(count_report.total_variant_count, 2);
    EXPECT_U64(count_report.count_complete, 0);
    EXPECT_U64(count_report.truncation_reason, CLR_BUILDUP_ENUMERATION_TRUNCATED);

    free(variants);
}void build_up_count_reports_truncation(void) {
    ClearraBoard64Layout layout = buildup_test_standard_10x2_layout();
    uint8_t columns[5] = {0, 2, 4, 6, 8};
    clr_packing_problem packing = buildup_test_buildup_packing_problem(2, 5, 0);
    const uint8_t pieces[5] = {
        CLR_PIECE_O,
        CLR_PIECE_O,
        CLR_PIECE_O,
        CLR_PIECE_O,
        CLR_PIECE_O,
    };
    buildup_test_set_packing_pieces(
        &packing,
        pieces,
        5,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);

    clr_buildup_problem problem =
        buildup_test_build_problem_from_candidate(packing, buildup_test_o_candidate_for_columns(layout, columns, 5));
    clr_buildup_count_limits limits = {0};
    clr_buildup_count_report report;

    limits.max_variants = 1;

    EXPECT_BUILDUP_STATUS(clr_buildup_count_variants(&problem, &limits, &report),
                          CLR_BUILDUP_ENUMERATION_TRUNCATED);
    EXPECT_U64(report.total_variant_count, 2);
    EXPECT_U64(report.search_complete, 0);
    EXPECT_U64(report.solution_exists, 1);
    EXPECT_U64(report.count_complete, 0);
    EXPECT_U64(report.trace_retained, 0);
    EXPECT_U64(report.retained_variant_count, 0);
    EXPECT_U64(report.no_variant_reason, CLR_BUILDUP_OK);
    EXPECT_U64(report.truncation_reason, CLR_BUILDUP_ENUMERATION_TRUNCATED);
}void enumerate_variants_retention_limit_preserves_complete_count(void) {
    ClearraBoard64Layout layout = buildup_test_standard_10x2_layout();
    uint8_t columns[5] = {0, 2, 4, 6, 8};
    clr_packing_problem packing = buildup_test_buildup_packing_problem(2, 5, 0);
    const uint8_t pieces[5] = {
        CLR_PIECE_O,
        CLR_PIECE_O,
        CLR_PIECE_O,
        CLR_PIECE_O,
        CLR_PIECE_O,
    };
    buildup_test_set_packing_pieces(
        &packing,
        pieces,
        5,
        CLR_PIECE_SOURCE_FIXED_QUEUE,
        CLR_SUPPLY_PROVENANCE_FIXED_SEQUENCE);

    clr_buildup_problem problem =
        buildup_test_build_problem_from_candidate(packing, buildup_test_o_candidate_for_columns(layout, columns, 5));
    clr_build_variant_buffer *variants =
        (clr_build_variant_buffer *)malloc(sizeof(clr_build_variant_buffer));
    clr_buildup_enumeration_limits enumerate_limits = {0};
    clr_buildup_count_limits count_limits = {0};
    clr_buildup_count_report report;

    EXPECT_TRUE(variants != 0);
    enumerate_limits.max_variants = 1;
    count_limits.max_variants = 1;

    EXPECT_BUILDUP_STATUS(clr_buildup_enumerate_variants(
                              &problem, &enumerate_limits, variants),
                          CLR_BUILDUP_OK);
    EXPECT_BUILDUP_STATUS(clr_buildup_count_variants(&problem, &count_limits, &report),
                          CLR_BUILDUP_ENUMERATION_TRUNCATED);
    EXPECT_U64(variants->count, 1);
    EXPECT_U64(variants->total_variant_count, 120);
    EXPECT_U64(variants->count_complete, 1);
    EXPECT_U64(variants->trace_retention_truncated, 1);
    EXPECT_U64(report.count_complete, 0);
    EXPECT_U64(report.truncation_reason, CLR_BUILDUP_ENUMERATION_TRUNCATED);

    free(variants);
}
