#include "packing_tests_support.h"

static ClearraPackingCandidateView identity_candidate(
    ClearraBoard64Layout layout,
    uint8_t piece,
    uint16_t operation_id,
    uint64_t mask) {
    ClearraPackingCandidateView candidate;
    clearra_packing_candidate_view_clear(&candidate);
    candidate.final_board = mask;
    candidate.shape_mask = mask;
    candidate.placed_count = 1u;
    candidate.pieces[0] = piece;
    candidate.rotations[0] = CLEARRA_ROTATION_SPAWN;
    candidate.operation_ids[0] = operation_id;
    candidate.operation_masks[0] = mask;
    candidate.shape_key = clearra_packing_shape_key(layout, mask);
    candidate.tiling_key = clearra_packing_tiling_key_with_piece_identity(
        layout,
        candidate.pieces,
        candidate.rotations,
        candidate.operation_masks,
        candidate.operation_deleted_row_masks,
        1u);
    candidate.operation_set_key = clearra_packing_operation_set_key(&candidate);
    return candidate;
}

void candidate_buffer_is_soa(void) {
    static ClearraPackingCandidateBuffer buffer;
    const ClearraBoard64Layout layout = packing_test_two_by_two_layout();
    const uint8_t pieces[1] = {CLR_PIECE_O};

    EXPECT_STATUS(
        clearra_packing_enumerator_cpu_generate(
            layout, clearra_board64_empty(), 2u, pieces, 1u, &buffer),
        CLEARRA_PACKING_OK);
    EXPECT_U64(buffer.count, 1u);
    EXPECT_U64(buffer.pieces[0][0], CLR_PIECE_O);
    EXPECT_TRUE(buffer.operation_masks[0][0] != 0u);
    EXPECT_TRUE(buffer.shape_keys[0] != 0u);
    EXPECT_TRUE(buffer.tiling_keys[0] != 0u);
    EXPECT_TRUE(buffer.operation_set_keys[0] != 0u);
}

void shape_key_stable(void) {
    const ClearraBoard64Layout layout = packing_test_standard_two_line_layout();
    const uint64_t mask = layout.all_cells_mask;
    EXPECT_U64(
        clearra_packing_shape_key(layout, mask),
        clearra_packing_shape_key(layout, mask));
    EXPECT_TRUE(
        clearra_packing_shape_key(layout, mask) !=
        clearra_packing_shape_key(layout, mask >> 1u));
}

void tiling_key_stable(void) {
    const ClearraBoard64Layout layout = packing_test_standard_two_line_layout();
    const uint64_t left[3] = {UINT64_C(0x3), UINT64_C(0xc), UINT64_C(0x30)};
    const uint64_t right[3] = {UINT64_C(0x30), UINT64_C(0x3), UINT64_C(0xc)};
    EXPECT_U64(
        clearra_packing_cell_partition_key(layout, left, 3u),
        clearra_packing_cell_partition_key(layout, right, 3u));
}

void same_masks_different_piece_identity_not_same_tiling_key(void) {
    const ClearraBoard64Layout layout = packing_test_standard_two_line_layout();
    const uint64_t masks[1] = {UINT64_C(0xf)};
    const uint8_t left[1] = {CLR_PIECE_O};
    const uint8_t right[1] = {CLR_PIECE_I};
    const uint8_t rotations[1] = {CLEARRA_ROTATION_SPAWN};
    const uint16_t deleted[1] = {0u};
    EXPECT_TRUE(
        clearra_packing_tiling_key_with_piece_identity(
            layout, left, rotations, masks, deleted, 1u) !=
        clearra_packing_tiling_key_with_piece_identity(
            layout, right, rotations, masks, deleted, 1u));
}

void custom_piece_same_mask_different_definition_not_same_tiling_key(void) {
    const ClearraBoard64Layout layout = packing_test_standard_two_line_layout();
    const uint64_t masks[1] = {UINT64_C(0xf)};
    const uint8_t left[1] = {42u};
    const uint8_t right[1] = {43u};
    const uint8_t rotations[1] = {CLEARRA_ROTATION_SPAWN};
    const uint16_t deleted[1] = {0u};
    EXPECT_TRUE(
        clearra_packing_tiling_key_with_piece_identity(
            layout, left, rotations, masks, deleted, 1u) !=
        clearra_packing_tiling_key_with_piece_identity(
            layout, right, rotations, masks, deleted, 1u));
}

void same_mask_different_piece_definition_not_same_tiling(void) {
    custom_piece_same_mask_different_definition_not_same_tiling_key();
}

void same_shape_different_tiling_not_deduped_by_shape(void) {
    static ClearraPackingCandidateBuffer buffer;
    const ClearraBoard64Layout layout = packing_test_standard_two_line_layout();
    const ClearraPackingCandidateView left =
        identity_candidate(layout, CLR_PIECE_O, 1u, UINT64_C(0xf));
    const ClearraPackingCandidateView right =
        identity_candidate(layout, CLR_PIECE_I, 2u, UINT64_C(0xf));
    bool inserted = false;

    EXPECT_U64(left.shape_key, right.shape_key);
    EXPECT_TRUE(left.tiling_key != right.tiling_key);
    clearra_packing_candidate_buffer_clear(&buffer);
    EXPECT_STATUS(
        clearra_packing_deduper_push_unique(&buffer, &left, 0, &inserted),
        CLEARRA_PACKING_OK);
    EXPECT_TRUE(inserted);
    EXPECT_STATUS(
        clearra_packing_deduper_push_unique(&buffer, &right, 0, &inserted),
        CLEARRA_PACKING_OK);
    EXPECT_TRUE(inserted);
    EXPECT_U64(buffer.count, 2u);
}

void shape_family_visual_grouping_does_not_drop_tiling_variant(void) {
    same_shape_different_tiling_not_deduped_by_shape();
}

void shape_key_does_not_drop_tiling_variant(void) {
    same_shape_different_tiling_not_deduped_by_shape();
}

void tiling_key_does_not_drop_build_variant(void) {
    const ClearraBoard64Layout layout = packing_test_standard_two_line_layout();
    ClearraPackingCandidateView left =
        identity_candidate(layout, CLR_PIECE_T, 10u, UINT64_C(0x1c));
    ClearraPackingCandidateView right = left;
    right.operation_ids[0] = 11u;
    right.operation_set_key = clearra_packing_operation_set_key(&right);
    EXPECT_U64(left.tiling_key, right.tiling_key);
    EXPECT_TRUE(left.operation_set_key != right.operation_set_key);
}

void operation_set_key_stable(void) {
    const ClearraBoard64Layout layout = packing_test_standard_two_line_layout();
    ClearraPackingCandidateView first =
        identity_candidate(layout, CLR_PIECE_O, 1u, UINT64_C(0xf));
    ClearraPackingCandidateView second = first;
    EXPECT_U64(
        clearra_packing_operation_set_key(&first),
        clearra_packing_operation_set_key(&second));
}

void hash_collision_exact_confirm_works(void) {
    static ClearraPackingCandidateBuffer buffer;
    const ClearraBoard64Layout layout = packing_test_standard_two_line_layout();
    const ClearraPackingCandidateView left =
        identity_candidate(layout, CLR_PIECE_O, 1u, UINT64_C(0xf));
    const ClearraPackingCandidateView right =
        identity_candidate(layout, CLR_PIECE_O, 1u, UINT64_C(0xf0));
    bool inserted = false;

    clearra_packing_candidate_buffer_clear(&buffer);
    EXPECT_STATUS(
        clearra_packing_deduper_push_unique(&buffer, &left, 0, &inserted),
        CLEARRA_PACKING_OK);
    EXPECT_TRUE(inserted);
    EXPECT_U64(
        clearra_packing_hash_bucket(left.operation_set_key, 1u),
        clearra_packing_hash_bucket(right.operation_set_key, 1u));
    EXPECT_FALSE(clearra_packing_hash_confirm_exact(&buffer, 0u, &right));
}
