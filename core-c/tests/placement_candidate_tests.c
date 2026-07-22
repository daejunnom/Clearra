#include "packing_tests_support.h"
static ClearraPlacementCandidate placement_candidate_variant(
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    uint16_t operation_id,
    uint64_t mask) {
    ClearraPlacementCandidate candidate;
    candidate.piece = piece;
    candidate.rotation = rotation;
    candidate.x = x;
    candidate.y = y;
    candidate.operation_id = operation_id;
    candidate.required_deleted_row_mask = 0u;
    candidate.mask = mask;
    return candidate;
}static ClearraPlacementCandidateList list_with_candidate(
    ClearraPlacementCandidate candidate) {
    ClearraPlacementCandidateList list;
    clearra_placement_candidate_list_clear(&list);
    EXPECT_STATUS(clearra_placement_candidate_list_push(&list, candidate),
                  CLEARRA_PACKING_OK);
    EXPECT_U64(list.count, 1u);
    return list;
}void placement_candidate_preserves_same_mask_different_rotation(void) {
    ClearraPlacementCandidate base = placement_candidate_variant(
        CLR_PIECE_I,
        CLEARRA_ROTATION_SPAWN,
        0,
        0,
        10u,
        UINT64_C(0xf));
    ClearraPlacementCandidate rotated = base;
    rotated.rotation = CLEARRA_ROTATION_RIGHT;
    rotated.operation_id = 11u;

    ClearraPlacementCandidateList list = list_with_candidate(base);
    EXPECT_STATUS(clearra_placement_candidate_list_push(&list, rotated),
                  CLEARRA_PACKING_OK);

    EXPECT_U64(list.count, 2u);
    EXPECT_U64(list.candidates[0].rotation, CLEARRA_ROTATION_SPAWN);
    EXPECT_U64(list.candidates[1].rotation, CLEARRA_ROTATION_RIGHT);
}void same_mask_different_operation_id_not_dropped_before_buildup(void) {
    ClearraPlacementCandidate base = placement_candidate_variant(
        CLR_PIECE_T,
        CLEARRA_ROTATION_SPAWN,
        4,
        0,
        20u,
        UINT64_C(0x1c0));
    ClearraPlacementCandidate alternate = base;
    alternate.operation_id = 21u;

    ClearraPlacementCandidateList list = list_with_candidate(base);
    EXPECT_STATUS(clearra_placement_candidate_list_push(&list, alternate),
                  CLEARRA_PACKING_OK);

    EXPECT_U64(list.count, 2u);
    EXPECT_U64(list.candidates[0].operation_id, 20u);
    EXPECT_U64(list.candidates[1].operation_id, 21u);
}void placement_geometry_class_retains_operation_variants(void) {
    ClearraPlacementCandidate base = placement_candidate_variant(
        CLR_PIECE_O,
        CLEARRA_ROTATION_SPAWN,
        0,
        0,
        30u,
        UINT64_C(0x303));
    ClearraPlacementCandidate different_x = base;
    different_x.x = 1;
    different_x.operation_id = 31u;
    ClearraPlacementCandidate different_y = base;
    different_y.y = 1;
    different_y.operation_id = 32u;

    ClearraPlacementCandidateList list = list_with_candidate(base);
    EXPECT_STATUS(clearra_placement_candidate_list_push(&list, different_x),
                  CLEARRA_PACKING_OK);
    EXPECT_STATUS(clearra_placement_candidate_list_push(&list, different_y),
                  CLEARRA_PACKING_OK);

    EXPECT_U64(list.count, 3u);
    EXPECT_U64(list.candidates[1].x, 1);
    EXPECT_U64(list.candidates[2].y, 1);
}void build_up_tries_next_operation_variant_when_first_variant_unreachable(void) {
    ClearraPlacementCandidate base = placement_candidate_variant(
        CLR_PIECE_S,
        CLEARRA_ROTATION_SPAWN,
        2,
        0,
        40u,
        UINT64_C(0x0f0));
    ClearraPlacementCandidate reachability_alternate = base;
    reachability_alternate.rotation = CLEARRA_ROTATION_RIGHT;
    reachability_alternate.operation_id = 41u;

    ClearraPlacementCandidateList list = list_with_candidate(base);
    EXPECT_STATUS(
        clearra_placement_candidate_list_push(&list, reachability_alternate),
        CLEARRA_PACKING_OK);

    EXPECT_U64(list.count, 2u);
    EXPECT_U64(list.candidates[1].operation_id, 41u);
}void kick_sensitive_replay_not_lost_by_mask_dedupe(void) {
    ClearraPlacementCandidate no_kick_variant = placement_candidate_variant(
        CLR_PIECE_T,
        CLEARRA_ROTATION_RIGHT,
        4,
        0,
        50u,
        UINT64_C(0x2e0));
    ClearraPlacementCandidate kick_variant = no_kick_variant;
    kick_variant.rotation = CLEARRA_ROTATION_LEFT;
    kick_variant.operation_id = 51u;

    ClearraPlacementCandidateList list = list_with_candidate(no_kick_variant);
    EXPECT_STATUS(clearra_placement_candidate_list_push(&list, kick_variant),
                  CLEARRA_PACKING_OK);

    EXPECT_U64(list.count, 2u);
    EXPECT_U64(list.candidates[0].operation_id, 50u);
    EXPECT_U64(list.candidates[1].operation_id, 51u);
}void custom_piece_same_mask_different_definition_not_deduped(void) {
    ClearraPlacementCandidate custom_a = placement_candidate_variant(
        42u,
        CLEARRA_ROTATION_SPAWN,
        0,
        0,
        60u,
        UINT64_C(0xf));
    ClearraPlacementCandidate custom_b = custom_a;
    custom_b.piece = 43u;
    custom_b.operation_id = 61u;

    ClearraPlacementCandidateList list = list_with_candidate(custom_a);
    EXPECT_STATUS(clearra_placement_candidate_list_push(&list, custom_b),
                  CLEARRA_PACKING_OK);

    EXPECT_U64(list.count, 2u);
    EXPECT_U64(list.candidates[0].piece, 42u);
    EXPECT_U64(list.candidates[1].piece, 43u);
}
