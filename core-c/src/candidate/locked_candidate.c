#include "candidate.h"

#include "../reachability/reachability.h"
static ClearraCandidateStatus candidate_status_from_reachability_status(
    ClearraReachabilityStatus status) {
    if (status == CLEARRA_REACHABILITY_OK) {
        return CLEARRA_CANDIDATE_OK;
    }
    if (status == CLEARRA_REACHABILITY_COLLISION) {
        return CLEARRA_CANDIDATE_COLLISION;
    }
    if (status == CLEARRA_REACHABILITY_UNREACHABLE) {
        return CLEARRA_CANDIDATE_UNREACHABLE;
    }
    if (status == CLEARRA_REACHABILITY_CAPACITY_EXCEEDED) {
        return CLEARRA_CANDIDATE_CAPACITY_EXCEEDED;
    }
    return CLEARRA_CANDIDATE_INVALID_ARGUMENT;
}static ClearraCandidateStatus candidate_status_from_board_status(
    ClearraBoard64Status status) {
    if (status == CLEARRA_BOARD64_OK) {
        return CLEARRA_CANDIDATE_OK;
    }
    if (status == CLEARRA_BOARD64_COLLISION) {
        return CLEARRA_CANDIDATE_COLLISION;
    }
    if (status == CLEARRA_BOARD64_OUT_OF_BOUNDS ||
        status == CLEARRA_BOARD64_MASK_OUTSIDE_LAYOUT) {
        return CLEARRA_CANDIDATE_OUT_OF_BOUNDS;
    }
    return CLEARRA_CANDIDATE_INVALID_ARGUMENT;
}static ClearraCandidateStatus placement_is_collision_free(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint64_t mask,
    bool *out_collision_free) {
    if (out_collision_free == 0) {
        return CLEARRA_CANDIDATE_INVALID_ARGUMENT;
    }
    *out_collision_free = false;

    bool collision = false;
    ClearraBoard64Status board_status =
        clearra_board64_collision(layout, board, mask, &collision);
    if (board_status != CLEARRA_BOARD64_OK) {
        return candidate_status_from_board_status(board_status);
    }
    *out_collision_free = !collision;
    return CLEARRA_CANDIDATE_OK;
}static ClearraCandidateStatus placement_is_grounded(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    bool *out_grounded) {
    if (out_grounded == 0) {
        return CLEARRA_CANDIDATE_INVALID_ARGUMENT;
    }
    *out_grounded = false;
    if (y == 0) {
        *out_grounded = true;
        return CLEARRA_CANDIDATE_OK;
    }

    uint64_t below_mask = 0;
    ClearraCandidateStatus mask_status = clearra_candidate_mask_for_piece(
        layout, piece, rotation, x, (int8_t)(y - 1), &below_mask);
    if (mask_status != CLEARRA_CANDIDATE_OK) {
        return mask_status;
    }

    bool below_collision_free = false;
    ClearraCandidateStatus collision_status = placement_is_collision_free(
        layout, board, below_mask, &below_collision_free);
    if (collision_status != CLEARRA_CANDIDATE_OK) {
        return collision_status;
    }
    *out_grounded = !below_collision_free;
    return CLEARRA_CANDIDATE_OK;
}static ClearraCandidateStatus append_reachable_locked_placements(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    bool allow_180,
    const ClearraReachabilityKickTable *kick_table,
    ClearraCandidateList *out_list) {
    if (!clearra_board64_layout_is_valid(layout) || out_list == 0) {
        return CLEARRA_CANDIDATE_INVALID_ARGUMENT;
    }
    clearra_candidate_list_clear(out_list);

    uint8_t rotation_count = clearra_candidate_unique_rotation_count(piece);
    if (rotation_count == 0) {
        return CLEARRA_CANDIDATE_INVALID_PIECE;
    }

    uint8_t mode = allow_180 ? CLEARRA_REACHABILITY_MODE_LOCKED_180
                             : CLEARRA_REACHABILITY_MODE_LOCKED;
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
        int8_t max_y = (int8_t)(layout.height - height);
        for (int8_t y = 0; y <= max_y; y++) {
            for (uint8_t x = 0; x <= max_x; x++) {
                uint64_t mask = 0;
                ClearraCandidateStatus mask_status =
                    clearra_candidate_mask_for_piece(layout, piece, rotation, (int8_t)x,
                                                     y, &mask);
                if (mask_status != CLEARRA_CANDIDATE_OK) {
                    return mask_status;
                }

                bool collision_free = false;
                ClearraCandidateStatus collision_status =
                    placement_is_collision_free(layout, board, mask, &collision_free);
                if (collision_status != CLEARRA_CANDIDATE_OK) {
                    return collision_status;
                }
                if (!collision_free) {
                    continue;
                }

                bool grounded = false;
                ClearraCandidateStatus grounded_status = placement_is_grounded(
                    layout, board, piece, rotation, (int8_t)x, y, &grounded);
                if (grounded_status != CLEARRA_CANDIDATE_OK) {
                    return grounded_status;
                }
                if (!grounded) {
                    continue;
                }

                ClearraReachabilityReport report;
                ClearraReachabilityStatus reachability_status = clearra_reachability_check(
                    layout, board, piece, rotation, (int8_t)x, y, mode, kick_table,
                    &report);
                if (reachability_status != CLEARRA_REACHABILITY_OK) {
                    return candidate_status_from_reachability_status(reachability_status);
                }
                if (!report.reachable) {
                    continue;
                }

                ClearraRotationTransitionKind transition =
                    CLEARRA_ROTATION_TRANSITION_NONE;
                ClearraCandidateStatus transition_status =
                    clearra_candidate_transition_kind(
                        CLEARRA_CANDIDATE_ROTATION_ZERO, rotation, &transition);
                if (transition_status != CLEARRA_CANDIDATE_OK) {
                    return transition_status;
                }
                if (report.used_180) {
                    transition = CLEARRA_ROTATION_TRANSITION_HALF_TURN;
                }

                ClearraCandidateOperation operation;
                operation.piece = piece;
                operation.rotation = rotation;
                operation.x = (int8_t)x;
                operation.y = y;
                operation.mask = mask;
                operation.transition_kind = (uint8_t)transition;
                operation.kick_index = 0;
                operation.kick_dx = 0;
                operation.kick_dy = 0;

                ClearraCandidateStatus push_status =
                    clearra_candidate_push_operation(out_list, operation);
                if (push_status != CLEARRA_CANDIDATE_OK) {
                    return push_status;
                }
            }
        }
    }

    return CLEARRA_CANDIDATE_OK;
}ClearraCandidateStatus clearra_locked_candidates_generate(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    ClearraCandidateList *out_list) {
    return clearra_locked_candidates_generate_with_kicks(layout, board, piece, 0, out_list);
}ClearraCandidateStatus clearra_locked_candidates_generate_with_kicks(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    const ClearraReachabilityKickTable *kick_table,
    ClearraCandidateList *out_list) {
    return append_reachable_locked_placements(
        layout, board, piece, false, kick_table, out_list);
}ClearraCandidateStatus clearra_locked180_candidates_generate_with_kicks(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    const ClearraReachabilityKickTable *kick_table,
    ClearraCandidateList *out_list) {
    return append_reachable_locked_placements(
        layout, board, piece, true, kick_table, out_list);
}