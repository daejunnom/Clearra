#include "rules.h"

static const uint8_t STANDARD_PIECES[CLEARRA_RULE_STANDARD_PIECE_COUNT] = {
    CLR_PIECE_I,
    CLR_PIECE_O,
    CLR_PIECE_T,
    CLR_PIECE_S,
    CLR_PIECE_Z,
    CLR_PIECE_J,
    CLR_PIECE_L,
};

static const uint8_t EIGHT_DIRECTION_TRANSITIONS[CLEARRA_RULE_90_TRANSITION_COUNT][2] = {
    {CLEARRA_RULE_ROTATION_SPAWN, CLEARRA_RULE_ROTATION_RIGHT},
    {CLEARRA_RULE_ROTATION_RIGHT, CLEARRA_RULE_ROTATION_REVERSE},
    {CLEARRA_RULE_ROTATION_REVERSE, CLEARRA_RULE_ROTATION_LEFT},
    {CLEARRA_RULE_ROTATION_LEFT, CLEARRA_RULE_ROTATION_SPAWN},
    {CLEARRA_RULE_ROTATION_SPAWN, CLEARRA_RULE_ROTATION_LEFT},
    {CLEARRA_RULE_ROTATION_LEFT, CLEARRA_RULE_ROTATION_REVERSE},
    {CLEARRA_RULE_ROTATION_REVERSE, CLEARRA_RULE_ROTATION_RIGHT},
    {CLEARRA_RULE_ROTATION_RIGHT, CLEARRA_RULE_ROTATION_SPAWN},
};
ClearraCompactKickSequence clearra_no_kick_sequence(void) {
    ClearraCompactKickSequence sequence;
    sequence.count = 1;
    sequence.offsets[0].dx = 0;
    sequence.offsets[0].dy = 0;
    return sequence;
}ClearraRuleStatus clearra_no_kick_table(ClearraCompactKickTable *out_table) {
    if (out_table == 0) {
        return CLEARRA_RULE_INVALID_ARGUMENT;
    }

    clearra_kick_table_clear(
        out_table,
        CLR_KICK_NO_KICK,
        CLR_RULE_NO_KICK,
        false,
        false);
    for (uint8_t piece_index = 0; piece_index < CLEARRA_RULE_STANDARD_PIECE_COUNT;
         piece_index++) {
        for (uint8_t transition_index = 0;
             transition_index < CLEARRA_RULE_90_TRANSITION_COUNT;
             transition_index++) {
            ClearraRuleStatus status = clearra_kick_table_push(
                out_table,
                STANDARD_PIECES[piece_index],
                EIGHT_DIRECTION_TRANSITIONS[transition_index][0],
                EIGHT_DIRECTION_TRANSITIONS[transition_index][1],
                clearra_no_kick_sequence());
            if (status != CLEARRA_RULE_OK) {
                return status;
            }
        }
    }

    return CLEARRA_RULE_OK;
}