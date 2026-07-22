#include "candidate.h"
static ClearraCandidateStatus append_landing(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    ClearraCandidateList *out_list) {
    uint8_t width = 0;
    uint8_t height = 0;
    ClearraCandidateStatus bounds_status =
        clearra_candidate_shape_bounds(piece, rotation, &width, &height);
    if (bounds_status != CLEARRA_CANDIDATE_OK) {
        return bounds_status;
    }
    if (height > layout.height) {
        return CLEARRA_CANDIDATE_OK;
    }

    int8_t max_y = (int8_t)(layout.height - height);
    bool found = false;
    int8_t landing_y = 0;
    uint64_t landing_mask = 0;
    for (int16_t y = max_y; y >= 0; y--) {
        uint64_t mask = 0;
        ClearraCandidateStatus mask_status =
            clearra_candidate_mask_for_piece(layout, piece, rotation, x, (int8_t)y, &mask);
        if (mask_status != CLEARRA_CANDIDATE_OK) {
            return mask_status;
        }

        bool collision = false;
        ClearraBoard64Status board_status =
            clearra_board64_collision(layout, board, mask, &collision);
        if (board_status != CLEARRA_BOARD64_OK) {
            return CLEARRA_CANDIDATE_INVALID_ARGUMENT;
        }
        if (collision) {
            break;
        }

        found = true;
        landing_y = (int8_t)y;
        landing_mask = mask;
    }

    if (!found) {
        return CLEARRA_CANDIDATE_OK;
    }

    ClearraRotationTransitionKind transition = CLEARRA_ROTATION_TRANSITION_NONE;
    ClearraCandidateStatus transition_status = clearra_candidate_transition_kind(
        CLEARRA_CANDIDATE_ROTATION_ZERO, rotation, &transition);
    if (transition_status != CLEARRA_CANDIDATE_OK) {
        return transition_status;
    }

    ClearraCandidateOperation operation;
    operation.piece = piece;
    operation.rotation = rotation;
    operation.x = x;
    operation.y = landing_y;
    operation.mask = landing_mask;
    operation.transition_kind = (uint8_t)transition;
    operation.kick_index = 0;
    operation.kick_dx = 0;
    operation.kick_dy = 0;
    return clearra_candidate_push_operation(out_list, operation);
}ClearraCandidateStatus clearra_harddrop_candidates_generate(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    ClearraCandidateList *out_list) {
    if (!clearra_board64_layout_is_valid(layout) || out_list == 0) {
        return CLEARRA_CANDIDATE_INVALID_ARGUMENT;
    }

    uint8_t rotation_count = clearra_candidate_unique_rotation_count(piece);
    if (rotation_count == 0) {
        return CLEARRA_CANDIDATE_INVALID_PIECE;
    }

    clearra_candidate_list_clear(out_list);
    for (uint8_t rotation = 0; rotation < rotation_count; rotation++) {
        uint8_t width = 0;
        uint8_t height = 0;
        ClearraCandidateStatus bounds_status =
            clearra_candidate_shape_bounds(piece, rotation, &width, &height);
        if (bounds_status != CLEARRA_CANDIDATE_OK) {
            return bounds_status;
        }
        if (width > layout.width || height > layout.height) {
            continue;
        }

        uint8_t max_x = (uint8_t)(layout.width - width);
        for (uint8_t x = 0; x <= max_x; x++) {
            ClearraCandidateStatus status =
                append_landing(layout, board, piece, rotation, (int8_t)x, out_list);
            if (status != CLEARRA_CANDIDATE_OK) {
                return status;
            }
        }
    }

    return CLEARRA_CANDIDATE_OK;
}