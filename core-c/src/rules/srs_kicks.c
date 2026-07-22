#include "rules.h"

#define SRS_90_KICK_COUNT 5u
#define SRS_PLUS_180_KICK_COUNT 6u
#define SRS_PLUS_I_180_KICK_COUNT 6u

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

static const uint8_t ONE_EIGHTY_TRANSITIONS[CLEARRA_RULE_180_TRANSITION_COUNT][2] = {
    {CLEARRA_RULE_ROTATION_SPAWN, CLEARRA_RULE_ROTATION_REVERSE},
    {CLEARRA_RULE_ROTATION_RIGHT, CLEARRA_RULE_ROTATION_LEFT},
    {CLEARRA_RULE_ROTATION_REVERSE, CLEARRA_RULE_ROTATION_SPAWN},
    {CLEARRA_RULE_ROTATION_LEFT, CLEARRA_RULE_ROTATION_RIGHT},
};

static ClearraCompactKickSequence sequence_from_values(
    const int8_t values[][2],
    uint8_t count) {
    ClearraCompactKickSequence sequence = {0};
    sequence.count = count;
    for (uint8_t index = 0; index < count; index++) {
        sequence.offsets[index].dx = values[index][0];
        sequence.offsets[index].dy = values[index][1];
    }
    return sequence;
}

static ClearraCompactKickSequence jlstz_sequence(
    uint8_t from_rotation,
    uint8_t to_rotation) {
    static const int8_t zr[SRS_90_KICK_COUNT][2] = {
        {0, 0}, {-1, 0}, {-1, 1}, {0, -2}, {-1, -2}};
    static const int8_t rt[SRS_90_KICK_COUNT][2] = {
        {0, 0}, {1, 0}, {1, -1}, {0, 2}, {1, 2}};
    static const int8_t tl[SRS_90_KICK_COUNT][2] = {
        {0, 0}, {1, 0}, {1, 1}, {0, -2}, {1, -2}};
    static const int8_t lt[SRS_90_KICK_COUNT][2] = {
        {0, 0}, {-1, 0}, {-1, -1}, {0, 2}, {-1, 2}};

    if ((from_rotation == CLEARRA_RULE_ROTATION_SPAWN &&
         to_rotation == CLEARRA_RULE_ROTATION_RIGHT) ||
        (from_rotation == CLEARRA_RULE_ROTATION_REVERSE &&
         to_rotation == CLEARRA_RULE_ROTATION_RIGHT)) {
        return sequence_from_values(zr, SRS_90_KICK_COUNT);
    }
    if ((from_rotation == CLEARRA_RULE_ROTATION_RIGHT &&
         to_rotation == CLEARRA_RULE_ROTATION_SPAWN) ||
        (from_rotation == CLEARRA_RULE_ROTATION_RIGHT &&
         to_rotation == CLEARRA_RULE_ROTATION_REVERSE)) {
        return sequence_from_values(rt, SRS_90_KICK_COUNT);
    }
    if ((from_rotation == CLEARRA_RULE_ROTATION_REVERSE &&
         to_rotation == CLEARRA_RULE_ROTATION_LEFT) ||
        (from_rotation == CLEARRA_RULE_ROTATION_SPAWN &&
         to_rotation == CLEARRA_RULE_ROTATION_LEFT)) {
        return sequence_from_values(tl, SRS_90_KICK_COUNT);
    }
    if ((from_rotation == CLEARRA_RULE_ROTATION_LEFT &&
         to_rotation == CLEARRA_RULE_ROTATION_REVERSE) ||
        (from_rotation == CLEARRA_RULE_ROTATION_LEFT &&
         to_rotation == CLEARRA_RULE_ROTATION_SPAWN)) {
        return sequence_from_values(lt, SRS_90_KICK_COUNT);
    }
    return clearra_no_kick_sequence();
}

static ClearraCompactKickSequence srs_i_sequence(
    uint8_t from_rotation,
    uint8_t to_rotation) {
    static const int8_t zr_l2[SRS_90_KICK_COUNT][2] = {
        {0, 0}, {-2, 0}, {1, 0}, {-2, -1}, {1, 2}};
    static const int8_t rz_tl[SRS_90_KICK_COUNT][2] = {
        {0, 0}, {2, 0}, {-1, 0}, {2, 1}, {-1, -2}};
    static const int8_t rt_zl[SRS_90_KICK_COUNT][2] = {
        {0, 0}, {-1, 0}, {2, 0}, {-1, 2}, {2, -1}};
    static const int8_t tr_lz[SRS_90_KICK_COUNT][2] = {
        {0, 0}, {1, 0}, {-2, 0}, {1, -2}, {-2, 1}};

    if ((from_rotation == CLEARRA_RULE_ROTATION_SPAWN &&
         to_rotation == CLEARRA_RULE_ROTATION_RIGHT) ||
        (from_rotation == CLEARRA_RULE_ROTATION_LEFT &&
         to_rotation == CLEARRA_RULE_ROTATION_REVERSE)) {
        return sequence_from_values(zr_l2, SRS_90_KICK_COUNT);
    }
    if ((from_rotation == CLEARRA_RULE_ROTATION_RIGHT &&
         to_rotation == CLEARRA_RULE_ROTATION_SPAWN) ||
        (from_rotation == CLEARRA_RULE_ROTATION_REVERSE &&
         to_rotation == CLEARRA_RULE_ROTATION_LEFT)) {
        return sequence_from_values(rz_tl, SRS_90_KICK_COUNT);
    }
    if ((from_rotation == CLEARRA_RULE_ROTATION_RIGHT &&
         to_rotation == CLEARRA_RULE_ROTATION_REVERSE) ||
        (from_rotation == CLEARRA_RULE_ROTATION_SPAWN &&
         to_rotation == CLEARRA_RULE_ROTATION_LEFT)) {
        return sequence_from_values(rt_zl, SRS_90_KICK_COUNT);
    }
    if ((from_rotation == CLEARRA_RULE_ROTATION_REVERSE &&
         to_rotation == CLEARRA_RULE_ROTATION_RIGHT) ||
        (from_rotation == CLEARRA_RULE_ROTATION_LEFT &&
         to_rotation == CLEARRA_RULE_ROTATION_SPAWN)) {
        return sequence_from_values(tr_lz, SRS_90_KICK_COUNT);
    }
    return clearra_no_kick_sequence();
}

static ClearraCompactKickSequence srs_plus_i_sequence(
    uint8_t from_rotation,
    uint8_t to_rotation) {
    static const int8_t zr[SRS_90_KICK_COUNT][2] = {
        {0, 0}, {1, 0}, {-2, 0}, {-2, -1}, {1, 2}};
    static const int8_t rz[SRS_90_KICK_COUNT][2] = {
        {0, 0}, {-1, 0}, {2, 0}, {-1, -2}, {2, 1}};
    static const int8_t rt[SRS_90_KICK_COUNT][2] = {
        {0, 0}, {-1, 0}, {2, 0}, {-1, 2}, {2, -1}};
    static const int8_t tr[SRS_90_KICK_COUNT][2] = {
        {0, 0}, {-2, 0}, {1, 0}, {-2, 1}, {1, -2}};
    static const int8_t zl[SRS_90_KICK_COUNT][2] = {
        {0, 0}, {-1, 0}, {2, 0}, {2, -1}, {-1, 2}};
    static const int8_t lz[SRS_90_KICK_COUNT][2] = {
        {0, 0}, {1, 0}, {-2, 0}, {1, -2}, {-2, 1}};
    static const int8_t lt[SRS_90_KICK_COUNT][2] = {
        {0, 0}, {1, 0}, {-2, 0}, {1, 2}, {-2, -1}};
    static const int8_t tl[SRS_90_KICK_COUNT][2] = {
        {0, 0}, {2, 0}, {-1, 0}, {2, 1}, {-1, -2}};

    if (from_rotation == CLEARRA_RULE_ROTATION_SPAWN &&
        to_rotation == CLEARRA_RULE_ROTATION_RIGHT) {
        return sequence_from_values(zr, SRS_90_KICK_COUNT);
    }
    if (from_rotation == CLEARRA_RULE_ROTATION_RIGHT &&
        to_rotation == CLEARRA_RULE_ROTATION_SPAWN) {
        return sequence_from_values(rz, SRS_90_KICK_COUNT);
    }
    if (from_rotation == CLEARRA_RULE_ROTATION_RIGHT &&
        to_rotation == CLEARRA_RULE_ROTATION_REVERSE) {
        return sequence_from_values(rt, SRS_90_KICK_COUNT);
    }
    if (from_rotation == CLEARRA_RULE_ROTATION_REVERSE &&
        to_rotation == CLEARRA_RULE_ROTATION_RIGHT) {
        return sequence_from_values(tr, SRS_90_KICK_COUNT);
    }
    if (from_rotation == CLEARRA_RULE_ROTATION_SPAWN &&
        to_rotation == CLEARRA_RULE_ROTATION_LEFT) {
        return sequence_from_values(zl, SRS_90_KICK_COUNT);
    }
    if (from_rotation == CLEARRA_RULE_ROTATION_LEFT &&
        to_rotation == CLEARRA_RULE_ROTATION_SPAWN) {
        return sequence_from_values(lz, SRS_90_KICK_COUNT);
    }
    if (from_rotation == CLEARRA_RULE_ROTATION_LEFT &&
        to_rotation == CLEARRA_RULE_ROTATION_REVERSE) {
        return sequence_from_values(lt, SRS_90_KICK_COUNT);
    }
    if (from_rotation == CLEARRA_RULE_ROTATION_REVERSE &&
        to_rotation == CLEARRA_RULE_ROTATION_LEFT) {
        return sequence_from_values(tl, SRS_90_KICK_COUNT);
    }
    return clearra_no_kick_sequence();
}

static ClearraCompactKickSequence srs_90_sequence(
    uint8_t piece,
    uint8_t from_rotation,
    uint8_t to_rotation,
    bool srs_plus) {
    if (piece == CLR_PIECE_O) {
        return clearra_no_kick_sequence();
    }
    if (piece == CLR_PIECE_I) {
        return srs_plus
                   ? srs_plus_i_sequence(from_rotation, to_rotation)
                   : srs_i_sequence(from_rotation, to_rotation);
    }
    return jlstz_sequence(from_rotation, to_rotation);
}

static ClearraCompactKickSequence srs_plus_180_sequence(
    uint8_t piece,
    uint8_t from_rotation,
    uint8_t to_rotation) {
    static const int8_t zt[SRS_PLUS_180_KICK_COUNT][2] = {
        {0, 0}, {0, 1}, {1, 1}, {-1, 1}, {1, 0}, {-1, 0}};
    static const int8_t tz[SRS_PLUS_180_KICK_COUNT][2] = {
        {0, 0}, {0, -1}, {-1, -1}, {1, -1}, {-1, 0}, {1, 0}};
    static const int8_t rl[SRS_PLUS_180_KICK_COUNT][2] = {
        {0, 0}, {1, 0}, {1, 2}, {1, 1}, {0, 2}, {0, 1}};
    static const int8_t lr[SRS_PLUS_180_KICK_COUNT][2] = {
        {0, 0}, {-1, 0}, {-1, 2}, {-1, 1}, {0, 2}, {0, 1}};
    static const int8_t i_zt[SRS_PLUS_I_180_KICK_COUNT][2] = {
        {0, 0}, {0, 1}, {1, 1}, {-1, 1}, {1, 0}, {-1, 0}};
    static const int8_t i_rl[SRS_PLUS_I_180_KICK_COUNT][2] = {
        {1, 1}, {1, 0}, {0, 0}, {2, 0}, {0, 1}, {2, 1}};
    static const int8_t i_tz[SRS_PLUS_I_180_KICK_COUNT][2] = {
        {-1, -1}, {0, -1}, {0, 1}, {0, 0}, {-1, 1}, {-1, 0}};
    static const int8_t i_lr[SRS_PLUS_I_180_KICK_COUNT][2] = {
        {0, 0}, {-1, 0}, {-1, 2}, {-1, 1}, {0, 2}, {0, 1}};

    if (piece == CLR_PIECE_O) {
        return (ClearraCompactKickSequence){0};
    }
    if (piece == CLR_PIECE_I) {
        if (from_rotation == CLEARRA_RULE_ROTATION_SPAWN &&
            to_rotation == CLEARRA_RULE_ROTATION_REVERSE) {
            return sequence_from_values(i_zt, SRS_PLUS_I_180_KICK_COUNT);
        }
        if (from_rotation == CLEARRA_RULE_ROTATION_RIGHT &&
            to_rotation == CLEARRA_RULE_ROTATION_LEFT) {
            return sequence_from_values(i_rl, SRS_PLUS_I_180_KICK_COUNT);
        }
        if (from_rotation == CLEARRA_RULE_ROTATION_REVERSE &&
            to_rotation == CLEARRA_RULE_ROTATION_SPAWN) {
            return sequence_from_values(i_tz, SRS_PLUS_I_180_KICK_COUNT);
        }
        if (from_rotation == CLEARRA_RULE_ROTATION_LEFT &&
            to_rotation == CLEARRA_RULE_ROTATION_RIGHT) {
            return sequence_from_values(i_lr, SRS_PLUS_I_180_KICK_COUNT);
        }
        return (ClearraCompactKickSequence){0};
    }
    if (from_rotation == CLEARRA_RULE_ROTATION_SPAWN &&
        to_rotation == CLEARRA_RULE_ROTATION_REVERSE) {
        return sequence_from_values(zt, SRS_PLUS_180_KICK_COUNT);
    }
    if (from_rotation == CLEARRA_RULE_ROTATION_REVERSE &&
        to_rotation == CLEARRA_RULE_ROTATION_SPAWN) {
        return sequence_from_values(tz, SRS_PLUS_180_KICK_COUNT);
    }
    if (from_rotation == CLEARRA_RULE_ROTATION_RIGHT &&
        to_rotation == CLEARRA_RULE_ROTATION_LEFT) {
        return sequence_from_values(rl, SRS_PLUS_180_KICK_COUNT);
    }
    if (from_rotation == CLEARRA_RULE_ROTATION_LEFT &&
        to_rotation == CLEARRA_RULE_ROTATION_RIGHT) {
        return sequence_from_values(lr, SRS_PLUS_180_KICK_COUNT);
    }
    return clearra_no_kick_sequence();
}

static ClearraRuleStatus append_srs_90_transitions(
    ClearraCompactKickTable *table,
    bool srs_plus) {
    for (uint8_t piece_index = 0; piece_index < CLEARRA_RULE_STANDARD_PIECE_COUNT;
         piece_index++) {
        for (uint8_t transition_index = 0;
             transition_index < CLEARRA_RULE_90_TRANSITION_COUNT;
             transition_index++) {
            uint8_t piece = STANDARD_PIECES[piece_index];
            uint8_t from_rotation = EIGHT_DIRECTION_TRANSITIONS[transition_index][0];
            uint8_t to_rotation = EIGHT_DIRECTION_TRANSITIONS[transition_index][1];
            ClearraRuleStatus status = clearra_kick_table_push(
                table,
                piece,
                from_rotation,
                to_rotation,
                srs_90_sequence(piece, from_rotation, to_rotation, srs_plus));
            if (status != CLEARRA_RULE_OK) {
                return status;
            }
        }
    }
    return CLEARRA_RULE_OK;
}

ClearraRuleStatus clearra_srs_kick_table(ClearraCompactKickTable *out_table) {
    if (out_table == 0) {
        return CLEARRA_RULE_INVALID_ARGUMENT;
    }
    clearra_kick_table_clear(out_table, CLR_KICK_SRS_90, CLR_RULE_SRS, false, false);
    return append_srs_90_transitions(out_table, false);
}

ClearraRuleStatus clearra_srs_plus_kick_table(ClearraCompactKickTable *out_table) {
    if (out_table == 0) {
        return CLEARRA_RULE_INVALID_ARGUMENT;
    }
    clearra_kick_table_clear(
        out_table,
        CLR_KICK_SRS_PLUS_180,
        CLR_RULE_SRS_PLUS,
        true,
        true);

    ClearraRuleStatus status = append_srs_90_transitions(out_table, true);
    if (status != CLEARRA_RULE_OK) {
        return status;
    }

    for (uint8_t piece_index = 0; piece_index < CLEARRA_RULE_STANDARD_PIECE_COUNT;
         piece_index++) {
        uint8_t piece = STANDARD_PIECES[piece_index];
        if (piece == CLR_PIECE_O) {
            continue;
        }
        for (uint8_t transition_index = 0;
             transition_index < CLEARRA_RULE_180_TRANSITION_COUNT;
             transition_index++) {
            uint8_t from_rotation = ONE_EIGHTY_TRANSITIONS[transition_index][0];
            uint8_t to_rotation = ONE_EIGHTY_TRANSITIONS[transition_index][1];
            status = clearra_kick_table_push(
                out_table,
                piece,
                from_rotation,
                to_rotation,
                srs_plus_180_sequence(piece, from_rotation, to_rotation));
            if (status != CLEARRA_RULE_OK) {
                return status;
            }
        }
    }

    return CLEARRA_RULE_OK;
}
