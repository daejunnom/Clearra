#include "reachability.h"
ClearraReachabilityStatus clearra_reachability_kick_offsets_for_transition(
    const ClearraReachabilityKickTable *kick_table,
    uint8_t from_rotation,
    uint8_t to_rotation,
    const ClearraKickOffset **out_offsets,
    uint8_t *out_count) {
    if (out_offsets == 0 || out_count == 0) {
        return CLEARRA_REACHABILITY_INVALID_ARGUMENT;
    }
    *out_offsets = 0;
    *out_count = 0;
    if (kick_table == 0) {
        return CLEARRA_REACHABILITY_UNREACHABLE;
    }

    ClearraRotationTransitionKind transition = CLEARRA_ROTATION_TRANSITION_NONE;
    ClearraCandidateStatus candidate_status =
        clearra_candidate_transition_kind(from_rotation, to_rotation, &transition);
    if (candidate_status != CLEARRA_CANDIDATE_OK) {
        return CLEARRA_REACHABILITY_INVALID_OPERATION;
    }

    if (kick_table->compact_table != 0) {
        const ClearraCompactKickSequence *sequence = 0;
        ClearraRuleStatus rule_status = clearra_kick_table_sequence_for(
            kick_table->compact_table, kick_table->piece, from_rotation, to_rotation,
            &sequence);
        if (rule_status == CLEARRA_RULE_TRANSITION_NOT_FOUND) {
            return CLEARRA_REACHABILITY_UNREACHABLE;
        }
        if (rule_status != CLEARRA_RULE_OK || sequence == 0) {
            return CLEARRA_REACHABILITY_INVALID_OPERATION;
        }
        *out_offsets = sequence->offsets;
        *out_count = sequence->count;
    } else if (transition == CLEARRA_ROTATION_TRANSITION_CLOCKWISE) {
        *out_offsets = kick_table->clockwise_offsets;
        *out_count = kick_table->clockwise_count;
    } else if (transition == CLEARRA_ROTATION_TRANSITION_COUNTER_CLOCKWISE) {
        *out_offsets = kick_table->counter_clockwise_offsets;
        *out_count = kick_table->counter_clockwise_count;
    } else if (transition == CLEARRA_ROTATION_TRANSITION_HALF_TURN) {
        *out_offsets = kick_table->half_turn_offsets;
        *out_count = kick_table->half_turn_count;
    }

    if (*out_offsets == 0 || *out_count == 0) {
        return CLEARRA_REACHABILITY_UNREACHABLE;
    }
    return CLEARRA_REACHABILITY_OK;
}ClearraReachabilityStatus clearra_kick_first_success(
    ClearraBoard64Layout layout,
    uint64_t board,
    uint8_t piece,
    uint8_t from_rotation,
    uint8_t to_rotation,
    int8_t anchor_x,
    int8_t anchor_y,
    const ClearraReachabilityKickTable *kick_table,
    ClearraCandidateOperation *out_operation) {
    const ClearraKickOffset *offsets = 0;
    uint8_t offset_count = 0;
    ClearraReachabilityStatus status = clearra_reachability_kick_offsets_for_transition(
        kick_table, from_rotation, to_rotation, &offsets, &offset_count);
    if (status != CLEARRA_REACHABILITY_OK) {
        return status;
    }

    ClearraCandidateStatus candidate_status = clearra_candidate_first_success_kick(
        layout, board, piece, from_rotation, to_rotation, anchor_x, anchor_y, offsets,
        offset_count, out_operation);
    if (candidate_status == CLEARRA_CANDIDATE_OK) {
        return CLEARRA_REACHABILITY_OK;
    }
    if (candidate_status == CLEARRA_CANDIDATE_UNREACHABLE) {
        return CLEARRA_REACHABILITY_UNREACHABLE;
    }
    if (candidate_status == CLEARRA_CANDIDATE_COLLISION) {
        return CLEARRA_REACHABILITY_COLLISION;
    }
    return CLEARRA_REACHABILITY_INVALID_OPERATION;
}