#include "packing_tests_support.h"
void same_operation_set_deduped(void) {
    static ClearraPackingCandidateBuffer raw;
    static ClearraCanonicalPackingTable table;
    ClearraBoard64Layout layout = packing_test_standard_two_line_layout();
    ClearraPackingCandidateView candidate =
        packing_test_single_operation_candidate(layout, UINT64_C(0x3), 0);

    clearra_packing_candidate_buffer_clear(&raw);
    packing_test_push_raw_candidate(&raw, candidate);
    packing_test_push_raw_candidate(&raw, candidate);

    EXPECT_STATUS(clearra_packing_host_reduce(&raw, &table), CLEARRA_PACKING_OK);
    EXPECT_U64(table.raw_count, 2);
    EXPECT_U64(table.candidates.count, 1);
    EXPECT_U64(table.raw_to_canonical_ids[0], table.raw_to_canonical_ids[1]);
    EXPECT_U64(table.candidate_ids[0], 0);
}
void different_operation_set_preserved(void) {
    static ClearraPackingCandidateBuffer raw;
    static ClearraCanonicalPackingTable table;
    ClearraBoard64Layout layout = packing_test_standard_two_line_layout();
    ClearraPackingCandidateView left =
        packing_test_single_operation_candidate(layout, UINT64_C(0x3), 0);
    ClearraPackingCandidateView right =
        packing_test_single_operation_candidate(layout, UINT64_C(0xc), 2);

    clearra_packing_candidate_buffer_clear(&raw);
    packing_test_push_raw_candidate(&raw, left);
    packing_test_push_raw_candidate(&raw, right);

    EXPECT_STATUS(clearra_packing_host_reduce(&raw, &table), CLEARRA_PACKING_OK);
    EXPECT_U64(table.raw_count, 2);
    EXPECT_U64(table.candidates.count, 2);
    EXPECT_TRUE(table.raw_to_canonical_ids[0] != table.raw_to_canonical_ids[1]);
}void packing_deduper_preserves_distinct_operation_sets(void) {
    different_operation_set_preserved();
}void candidate_identity_includes_final_board_and_cleared_lines(void) {
    static ClearraPackingCandidateBuffer raw;
    static ClearraCanonicalPackingTable table;
    ClearraBoard64Layout layout = packing_test_standard_two_line_layout();
    ClearraPackingCandidateView left =
        packing_test_single_operation_candidate(layout, UINT64_C(0x3), 0);
    ClearraPackingCandidateView right = left;

    right.final_board = UINT64_C(0);
    right.cleared_lines = 1;

    EXPECT_TRUE(clearra_packing_candidate_identity_key(&left) !=
                clearra_packing_candidate_identity_key(&right));

    clearra_packing_candidate_buffer_clear(&raw);
    packing_test_push_raw_candidate(&raw, left);
    packing_test_push_raw_candidate(&raw, right);

    EXPECT_STATUS(clearra_packing_host_reduce(&raw, &table), CLEARRA_PACKING_OK);
    EXPECT_U64(table.raw_count, 2);
    EXPECT_U64(table.candidates.count, 2);
    EXPECT_TRUE(table.raw_to_canonical_ids[0] != table.raw_to_canonical_ids[1]);
}void hash_collision_preserved_by_exact_confirm(void) {
    static ClearraPackingCandidateBuffer raw;
    static ClearraCanonicalPackingTable table;
    ClearraBoard64Layout layout = packing_test_standard_two_line_layout();
    ClearraPackingCandidateView left =
        packing_test_single_operation_candidate(layout, UINT64_C(0x3), 0);
    ClearraPackingCandidateView right =
        packing_test_single_operation_candidate(layout, UINT64_C(0xc), 2);

    right.shape_key = left.shape_key;
    right.tiling_key = left.tiling_key;
    right.operation_set_key = left.operation_set_key;

    clearra_packing_candidate_buffer_clear(&raw);
    packing_test_push_raw_candidate(&raw, left);
    packing_test_push_raw_candidate(&raw, right);

    EXPECT_U64(left.shape_key, right.shape_key);
    EXPECT_U64(left.tiling_key, right.tiling_key);
    EXPECT_U64(left.operation_set_key, right.operation_set_key);
    EXPECT_FALSE(clearra_packing_hash_confirm_same_operation_set(&raw, 0, &right));
    EXPECT_STATUS(clearra_packing_host_reduce(&raw, &table), CLEARRA_PACKING_OK);
    EXPECT_U64(table.candidates.count, 2);
    EXPECT_TRUE(table.raw_to_canonical_ids[0] != table.raw_to_canonical_ids[1]);
}
void candidate_ids_stable_across_cpu_gpu_backend_order(void) {
    static ClearraPackingCandidateBuffer cpu_raw;
    static ClearraPackingCandidateBuffer gpu_raw;
    static ClearraCanonicalPackingTable cpu_table;
    static ClearraCanonicalPackingTable gpu_table;
    ClearraBoard64Layout layout = packing_test_standard_two_line_layout();
    ClearraPackingCandidateView left =
        packing_test_single_operation_candidate(layout, UINT64_C(0x3), 0);
    ClearraPackingCandidateView right =
        packing_test_single_operation_candidate(layout, UINT64_C(0xc), 2);

    clearra_packing_candidate_buffer_clear(&cpu_raw);
    packing_test_push_raw_candidate(&cpu_raw, left);
    packing_test_push_raw_candidate(&cpu_raw, right);

    clearra_packing_candidate_buffer_clear(&gpu_raw);
    packing_test_push_raw_candidate(&gpu_raw, right);
    packing_test_push_raw_candidate(&gpu_raw, left);

    EXPECT_STATUS(clearra_packing_host_reduce(&cpu_raw, &cpu_table),
                  CLEARRA_PACKING_OK);
    EXPECT_STATUS(clearra_packing_host_reduce(&gpu_raw, &gpu_table),
                  CLEARRA_PACKING_OK);

    EXPECT_U64(cpu_table.candidates.count, gpu_table.candidates.count);
    EXPECT_U64(cpu_table.candidate_ids[0], 0);
    EXPECT_U64(cpu_table.candidate_ids[1], 1);
    EXPECT_U64(gpu_table.candidate_ids[0], 0);
    EXPECT_U64(gpu_table.candidate_ids[1], 1);
    EXPECT_U64(cpu_table.candidates.operation_set_keys[0],
               gpu_table.candidates.operation_set_keys[0]);
    EXPECT_U64(cpu_table.candidates.operation_set_keys[1],
               gpu_table.candidates.operation_set_keys[1]);
    EXPECT_U64(cpu_table.raw_to_canonical_ids[0], gpu_table.raw_to_canonical_ids[1]);
    EXPECT_U64(cpu_table.raw_to_canonical_ids[1], gpu_table.raw_to_canonical_ids[0]);
}void candidate_buffer_exports_canonical_identity(void) {
    static ClearraPackingCandidateBuffer buffer;
    ClearraBoard64Layout layout = packing_test_standard_two_line_layout();
    ClearraPackingCandidateView first =
        packing_test_single_operation_candidate(layout, UINT64_C(0x3), 0);
    ClearraPackingCandidateView second =
        packing_test_single_operation_candidate(layout, UINT64_C(0xc), 2);
    ClearraPackingCandidateView exported;

    clearra_packing_candidate_buffer_clear(&buffer);
    packing_test_push_raw_candidate(&buffer, first);
    packing_test_push_raw_candidate(&buffer, second);

    EXPECT_STATUS(clearra_packing_candidate_buffer_candidate_at(
                      &buffer, 1u, &exported),
                  CLEARRA_PACKING_OK);
    EXPECT_U64(exported.candidate_id, UINT64_C(2));
    EXPECT_U64(exported.canonical_operation_set_id, UINT64_C(2));
}