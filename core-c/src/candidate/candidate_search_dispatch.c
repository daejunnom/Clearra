#include "candidate.h"
#include "candidate_generator_internal.h"

#include "../piece/operation.h"
#include "../reachability/reachability.h"

static ClearraCandidateStatus normalized_rotation_center(
    uint8_t piece,
    uint8_t rotation,
    int8_t *out_x,
    int8_t *out_y) {
    static const int8_t jlstz_centers[CLEARRA_ROTATION_STATE_COUNT][2] = {
        {1, 0}, {0, 1}, {1, 1}, {1, 1}};
    static const int8_t i_centers[CLEARRA_ROTATION_STATE_COUNT][2] = {
        {0, 0}, {-2, 2}, {0, 1}, {-1, 2}};
    if (out_x == 0 || out_y == 0 ||
        rotation >= CLEARRA_ROTATION_STATE_COUNT) {
        return CLEARRA_CANDIDATE_INVALID_ARGUMENT;
    }
    if (piece == CLR_PIECE_I) {
        *out_x = i_centers[rotation][0];
        *out_y = i_centers[rotation][1];
        return CLEARRA_CANDIDATE_OK;
    }
    if (piece == CLR_PIECE_O) {
        *out_x = 0;
        *out_y = 0;
        return CLEARRA_CANDIDATE_OK;
    }
    if (piece >= CLR_PIECE_T && piece <= CLR_PIECE_L) {
        *out_x = jlstz_centers[rotation][0];
        *out_y = jlstz_centers[rotation][1];
        return CLEARRA_CANDIDATE_OK;
    }
    return CLEARRA_CANDIDATE_INVALID_PIECE;
}

ClearraCandidateStatus clearra_candidate_normalized_kick_delta(
    uint8_t piece,
    uint8_t from_rotation,
    uint8_t to_rotation,
    int8_t kick_dx,
    int8_t kick_dy,
    int8_t *out_dx,
    int8_t *out_dy) {
    int8_t from_center_x = 0;
    int8_t from_center_y = 0;
    int8_t to_center_x = 0;
    int8_t to_center_y = 0;
    ClearraCandidateStatus status = normalized_rotation_center(
        piece, from_rotation, &from_center_x, &from_center_y);
    if (status != CLEARRA_CANDIDATE_OK) {
        return status;
    }
    status = normalized_rotation_center(
        piece, to_rotation, &to_center_x, &to_center_y);
    if (status != CLEARRA_CANDIDATE_OK || out_dx == 0 || out_dy == 0) {
        return status == CLEARRA_CANDIDATE_OK
            ? CLEARRA_CANDIDATE_INVALID_ARGUMENT
            : status;
    }
    *out_dx = (int8_t)(kick_dx + from_center_x - to_center_x);
    *out_dy = (int8_t)(kick_dy + from_center_y - to_center_y);
    return CLEARRA_CANDIDATE_OK;
}

void clearra_candidate_list_clear(ClearraCandidateList *list) {
    if (list != 0) {
        list->count = 0;
    }
}

ClearraCandidateStatus clearra_candidate_search(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t active_piece,
    const ClearraCompactRuleProfile *rule,
    uint8_t mode,
    ClearraCandidateList *out_list) {
    if (rule == 0) {
        return CLEARRA_CANDIDATE_INVALID_ARGUMENT;
    }
    if (mode == CLEARRA_CANDIDATE_MODE_HARDDROP ||
        rule->rule_profile_id == CLR_RULE_NO_KICK) {
        return clearra_harddrop_candidates_generate(layout, board, active_piece, out_list);
    }

    ClearraReachabilityKickTable kick_table;
    kick_table.compact_table = &rule->kick_table;
    kick_table.piece = active_piece;
    kick_table.clockwise_offsets = 0;
    kick_table.clockwise_count = 0;
    kick_table.counter_clockwise_offsets = 0;
    kick_table.counter_clockwise_count = 0;
    kick_table.half_turn_offsets = 0;
    kick_table.half_turn_count = 0;

    if (mode == CLEARRA_CANDIDATE_MODE_LOCKED_180) {
        if (!rule->supports_180) {
            return CLEARRA_CANDIDATE_UNREACHABLE;
        }
        return clearra_locked180_candidates_generate_with_kicks(
            layout, board, active_piece, &kick_table, out_list);
    }
    if (mode == CLEARRA_CANDIDATE_MODE_LOCKED) {
        return clearra_locked_candidates_generate_with_kicks(
            layout, board, active_piece, &kick_table, out_list);
    }
    return CLEARRA_CANDIDATE_INVALID_ARGUMENT;
}

#include "candidate.h"
#include "candidate_generator_internal.h"

#include "../piece/operation.h"
#include "../reachability/reachability.h"

ClearraCandidateStatus clearra_candidate_first_success_kick(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t from_rotation,
    uint8_t to_rotation,
    int8_t anchor_x,
    int8_t anchor_y,
    const ClearraKickOffset *offsets,
    uint8_t offset_count,
    ClearraCandidateOperation *out_operation) {
    if (offsets == 0 || offset_count == 0 || out_operation == 0) {
        return CLEARRA_CANDIDATE_INVALID_ARGUMENT;
    }

    ClearraRotationTransitionKind transition = CLEARRA_ROTATION_TRANSITION_NONE;
    ClearraCandidateStatus transition_status =
        clearra_candidate_transition_kind(from_rotation, to_rotation, &transition);
    if (transition_status != CLEARRA_CANDIDATE_OK) {
        return transition_status;
    }

    for (uint8_t index = 0; index < offset_count; index++) {
        int8_t normalized_dx = 0;
        int8_t normalized_dy = 0;
        ClearraCandidateStatus origin_status =
            clearra_candidate_normalized_kick_delta(
                piece, from_rotation, to_rotation, offsets[index].dx,
                offsets[index].dy, &normalized_dx, &normalized_dy);
        if (origin_status != CLEARRA_CANDIDATE_OK) {
            return origin_status;
        }
        int8_t candidate_x = (int8_t)(anchor_x + normalized_dx);
        int8_t candidate_y = (int8_t)(anchor_y + normalized_dy);
        uint64_t mask = 0;
        ClearraCandidateStatus mask_status =
            clearra_candidate_mask_for_piece(layout, piece, to_rotation, candidate_x,
                                             candidate_y, &mask);
        if (mask_status == CLEARRA_CANDIDATE_OUT_OF_BOUNDS) {
            continue;
        }
        if (mask_status != CLEARRA_CANDIDATE_OK) {
            return mask_status;
        }

        bool collision = false;
        ClearraBoard64Status board_status =
            clearra_board64_collision(layout, board, mask, &collision);
        if (board_status != CLEARRA_BOARD64_OK) {
            return clearra_candidate_status_from_board_status(board_status);
        }
        if (collision) {
            continue;
        }

        out_operation->piece = piece;
        out_operation->rotation = to_rotation;
        out_operation->x = candidate_x;
        out_operation->y = candidate_y;
        out_operation->mask = mask;
        out_operation->transition_kind = (uint8_t)transition;
        out_operation->kick_index = index;
        out_operation->kick_dx = offsets[index].dx;
        out_operation->kick_dy = offsets[index].dy;
        return CLEARRA_CANDIDATE_OK;
    }

    return CLEARRA_CANDIDATE_UNREACHABLE;
}

#include "candidate.h"
#include "candidate_generator_internal.h"

#include "../piece/operation.h"
#include "../reachability/reachability.h"

ClearraCandidateStatus clearra_candidate_mask_for_piece(
    ClearraBoard64Layout layout,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    uint64_t *out_mask) {
    ClearraOperation operation;
    ClearraOperationStatus status =
        clearra_operation_from_shape(piece, rotation, &operation);
    if (status != CLEARRA_OPERATION_OK) {
        return clearra_candidate_status_from_operation_status(status);
    }
    return clearra_candidate_status_from_operation_status(
        clearra_operation_mask(layout, &operation, x, y, out_mask));
}

#include "candidate.h"
#include "candidate_generator_internal.h"

#include "../piece/operation.h"
#include "../reachability/reachability.h"

ClearraCandidateStatus clearra_candidate_push_operation(
    ClearraCandidateList *list,
    ClearraCandidateOperation operation) {
    if (list == 0) {
        return CLEARRA_CANDIDATE_INVALID_ARGUMENT;
    }
    for (uint16_t index = 0; index < list->count; index++) {
        const ClearraCandidateOperation *existing = &list->operations[index];
        if (existing->piece == operation.piece &&
            existing->rotation == operation.rotation &&
            existing->x == operation.x &&
            existing->y == operation.y &&
            existing->mask == operation.mask) {
            return CLEARRA_CANDIDATE_OK;
        }
    }
    if (list->count >= CLEARRA_CANDIDATE_MAX_OPERATIONS) {
        return CLEARRA_CANDIDATE_CAPACITY_EXCEEDED;
    }
    list->operations[list->count] = operation;
    list->count++;
    return CLEARRA_CANDIDATE_OK;
}

#include "candidate.h"
#include "candidate_generator_internal.h"

#include "../piece/operation.h"
#include "../reachability/reachability.h"

ClearraCandidateStatus clearra_candidate_is_reachable_operation(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t rotation,
    int8_t x,
    int8_t y,
    bool *out_reachable) {
    if (out_reachable == 0) {
        return CLEARRA_CANDIDATE_INVALID_ARGUMENT;
    }
    *out_reachable = false;

    uint64_t mask = 0;
    ClearraCandidateStatus status =
        clearra_candidate_mask_for_piece(layout, piece, rotation, x, y, &mask);
    if (status != CLEARRA_CANDIDATE_OK) {
        return status;
    }

    bool collision = false;
    ClearraBoard64Status board_status =
        clearra_board64_collision(layout, board, mask, &collision);
    if (board_status != CLEARRA_BOARD64_OK) {
        return clearra_candidate_status_from_board_status(board_status);
    }
    if (collision) {
        return CLEARRA_CANDIDATE_COLLISION;
    }
    if (y == 0) {
        *out_reachable = true;
        return CLEARRA_CANDIDATE_OK;
    }

    uint64_t below_mask = 0;
    status = clearra_candidate_mask_for_piece(layout, piece, rotation, x, (int8_t)(y - 1),
                                              &below_mask);
    if (status == CLEARRA_CANDIDATE_OUT_OF_BOUNDS) {
        *out_reachable = true;
        return CLEARRA_CANDIDATE_OK;
    }
    if (status != CLEARRA_CANDIDATE_OK) {
        return status;
    }
    board_status = clearra_board64_collision(layout, board, below_mask, &collision);
    if (board_status != CLEARRA_BOARD64_OK) {
        return clearra_candidate_status_from_board_status(board_status);
    }
    *out_reachable = collision;
    return CLEARRA_CANDIDATE_OK;
}

#include "candidate.h"
#include "candidate_generator_internal.h"

#include "../piece/operation.h"
#include "../reachability/reachability.h"

ClearraCandidateStatus clearra_candidate_shape_bounds(
    uint8_t piece,
    uint8_t rotation,
    uint8_t *out_width,
    uint8_t *out_height) {
    if (out_width == 0 || out_height == 0) {
        return CLEARRA_CANDIDATE_INVALID_ARGUMENT;
    }
    ClearraOperation operation;
    ClearraOperationStatus status =
        clearra_operation_from_shape(piece, rotation, &operation);
    if (status != CLEARRA_OPERATION_OK) {
        return clearra_candidate_status_from_operation_status(status);
    }
    *out_width = operation.bounds.width;
    *out_height = operation.bounds.height;
    return CLEARRA_CANDIDATE_OK;
}

#include "candidate.h"
#include "candidate_generator_internal.h"

#include "../piece/operation.h"
#include "../reachability/reachability.h"

ClearraCandidateStatus clearra_candidate_status_from_board_status(
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
}

#include "candidate.h"
#include "candidate_generator_internal.h"

#include "../piece/operation.h"
#include "../reachability/reachability.h"

ClearraCandidateStatus clearra_candidate_status_from_operation_status(
    ClearraOperationStatus status) {
    if (status == CLEARRA_OPERATION_OK) {
        return CLEARRA_CANDIDATE_OK;
    }
    if (status == CLEARRA_OPERATION_INVALID_PIECE) {
        return CLEARRA_CANDIDATE_INVALID_PIECE;
    }
    if (status == CLEARRA_OPERATION_INVALID_ROTATION) {
        return CLEARRA_CANDIDATE_INVALID_ROTATION;
    }
    if (status == CLEARRA_OPERATION_OUT_OF_BOUNDS) {
        return CLEARRA_CANDIDATE_OUT_OF_BOUNDS;
    }
    return CLEARRA_CANDIDATE_INVALID_ARGUMENT;
}

#include "candidate.h"
#include "candidate_generator_internal.h"

#include "../piece/operation.h"
#include "../reachability/reachability.h"

ClearraCandidateStatus clearra_candidate_transition_kind(
    uint8_t from_rotation,
    uint8_t to_rotation,
    ClearraRotationTransitionKind *out_kind) {
    if (out_kind == 0) {
        return CLEARRA_CANDIDATE_INVALID_ARGUMENT;
    }
    if (from_rotation > CLEARRA_CANDIDATE_ROTATION_LEFT ||
        to_rotation > CLEARRA_CANDIDATE_ROTATION_LEFT) {
        return CLEARRA_CANDIDATE_INVALID_ROTATION;
    }

    uint8_t delta = (uint8_t)((to_rotation + 4u - from_rotation) % 4u);
    if (delta == 0) {
        *out_kind = CLEARRA_ROTATION_TRANSITION_NONE;
    } else if (delta == 1) {
        *out_kind = CLEARRA_ROTATION_TRANSITION_CLOCKWISE;
    } else if (delta == 2) {
        *out_kind = CLEARRA_ROTATION_TRANSITION_HALF_TURN;
    } else {
        *out_kind = CLEARRA_ROTATION_TRANSITION_COUNTER_CLOCKWISE;
    }
    return CLEARRA_CANDIDATE_OK;
}

#include "candidate.h"
#include "candidate_generator_internal.h"

#include "../piece/operation.h"
#include "../reachability/reachability.h"

uint8_t clearra_candidate_unique_rotation_count(uint8_t piece) {
    if (piece == CLEARRA_CANDIDATE_PIECE_O) {
        return 1;
    }
    if (piece == CLEARRA_CANDIDATE_PIECE_I ||
        piece == CLEARRA_CANDIDATE_PIECE_S ||
        piece == CLEARRA_CANDIDATE_PIECE_Z) {
        return 2;
    }
    if (piece == CLEARRA_CANDIDATE_PIECE_T ||
        piece == CLEARRA_CANDIDATE_PIECE_J ||
        piece == CLEARRA_CANDIDATE_PIECE_L) {
        return 4;
    }
    return 0;
}

#include "candidate.h"

ClearraCandidateStatus clearra_locked180_candidates_generate(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    ClearraCandidateList *out_list) {
    return clearra_locked180_candidates_generate_with_kicks(
        layout, board, piece, 0, out_list);
}
