#include "rules.h"
bool clearra_rule_rotation_is_valid(uint8_t rotation) {
    return rotation < CLEARRA_RULE_ROTATION_COUNT;
}bool clearra_rule_transition_is_180(uint8_t from_rotation, uint8_t to_rotation) {
    if (!clearra_rule_rotation_is_valid(from_rotation) ||
        !clearra_rule_rotation_is_valid(to_rotation)) {
        return false;
    }
    return ((to_rotation + CLEARRA_RULE_ROTATION_COUNT - from_rotation) %
            CLEARRA_RULE_ROTATION_COUNT) == 2u;
}void clearra_kick_table_clear(
    ClearraCompactKickTable *table,
    uint32_t kick_profile_id,
    uint32_t source_rule_profile_id,
    bool supports_180,
    bool srs_plus_capability_reported) {
    if (table == 0) {
        return;
    }
    table->kick_profile_id = kick_profile_id;
    table->source_rule_profile_id = source_rule_profile_id;
    table->supports_180 = supports_180;
    table->srs_plus_capability_reported = srs_plus_capability_reported;
    table->transition_count = 0;
}ClearraRuleStatus clearra_kick_table_push(
    ClearraCompactKickTable *table,
    uint8_t piece,
    uint8_t from_rotation,
    uint8_t to_rotation,
    ClearraCompactKickSequence sequence) {
    if (table == 0 || sequence.count == 0 ||
        sequence.count > CLEARRA_RULE_MAX_KICK_OFFSETS) {
        return CLEARRA_RULE_INVALID_ARGUMENT;
    }
    if (piece < CLR_PIECE_I || piece > CLR_PIECE_L) {
        return CLEARRA_RULE_INVALID_ARGUMENT;
    }
    if (!clearra_rule_rotation_is_valid(from_rotation) ||
        !clearra_rule_rotation_is_valid(to_rotation) || from_rotation == to_rotation) {
        return CLEARRA_RULE_INVALID_ARGUMENT;
    }
    if (clearra_rule_transition_is_180(from_rotation, to_rotation) &&
        !table->supports_180) {
        return CLEARRA_RULE_UNSUPPORTED_KICK_PROFILE;
    }
    if (table->transition_count >= CLEARRA_RULE_MAX_KICK_TRANSITIONS) {
        return CLEARRA_RULE_TABLE_CAPACITY_EXCEEDED;
    }

    ClearraCompactKickTransition *transition =
        &table->transitions[table->transition_count];
    transition->piece = piece;
    transition->from_rotation = from_rotation;
    transition->to_rotation = to_rotation;
    transition->sequence = sequence;
    table->transition_count++;
    return CLEARRA_RULE_OK;
}ClearraRuleStatus clearra_kick_table_sequence_for(
    const ClearraCompactKickTable *table,
    uint8_t piece,
    uint8_t from_rotation,
    uint8_t to_rotation,
    const ClearraCompactKickSequence **out_sequence) {
    if (table == 0 || out_sequence == 0) {
        return CLEARRA_RULE_INVALID_ARGUMENT;
    }
    for (uint16_t index = 0; index < table->transition_count; index++) {
        const ClearraCompactKickTransition *transition = &table->transitions[index];
        if (transition->piece == piece && transition->from_rotation == from_rotation &&
            transition->to_rotation == to_rotation) {
            *out_sequence = &transition->sequence;
            return CLEARRA_RULE_OK;
        }
    }
    return CLEARRA_RULE_TRANSITION_NOT_FOUND;
}bool clearra_kick_table_supports_180(const ClearraCompactKickTable *table) {
    return table != 0 && table->supports_180;
}bool clearra_kick_table_zero_offsets_only(const ClearraCompactKickTable *table) {
    if (table == 0 || table->transition_count == 0) {
        return false;
    }
    for (uint16_t transition_index = 0; transition_index < table->transition_count;
         transition_index++) {
        const ClearraCompactKickSequence *sequence =
            &table->transitions[transition_index].sequence;
        for (uint8_t offset_index = 0; offset_index < sequence->count; offset_index++) {
            if (sequence->offsets[offset_index].dx != 0 ||
                sequence->offsets[offset_index].dy != 0) {
                return false;
            }
        }
    }
    return true;
}