#include "../src/rules/rules.h"

#include <stdio.h>
#include <stdlib.h>

_Static_assert(CLR_RULE_MAX_KICK_OFFSETS == 12u,
               "public kick-sequence capacity must preserve SRS-X");
_Static_assert(CLEARRA_RULE_MAX_KICK_OFFSETS == CLR_RULE_MAX_KICK_OFFSETS,
               "public and compact kick-sequence capacities must agree");
_Static_assert(sizeof(clr_kick_offset_descriptor) == 2u,
               "public kick-offset ABI layout changed");
_Static_assert(sizeof(clr_kick_sequence_descriptor) == 28u,
               "public kick-sequence ABI layout changed");
_Static_assert(sizeof(clr_kick_transition_descriptor) == 32u,
               "public kick-transition ABI layout changed");
_Static_assert(sizeof(clr_rule_profile_descriptor) == 2712u,
               "public rule-profile ABI layout changed");

#define EXPECT_STATUS(EXPR, EXPECTED)                                                   \
    do {                                                                                \
        ClearraRuleStatus actual_status = (EXPR);                                       \
        if (actual_status != (EXPECTED)) {                                              \
            fprintf(stderr, "%s:%d expected status %d but got %d\n", __FILE__, __LINE__, \
                    (int)(EXPECTED), (int)actual_status);                               \
            exit(1);                                                                    \
        }                                                                               \
    } while (0)

#define EXPECT_TRUE(EXPR)                                                               \
    do {                                                                                \
        if (!(EXPR)) {                                                                  \
            fprintf(stderr, "%s:%d expected true\n", __FILE__, __LINE__);              \
            exit(1);                                                                    \
        }                                                                               \
    } while (0)

#define EXPECT_FALSE(EXPR)                                                              \
    do {                                                                                \
        if ((EXPR)) {                                                                   \
            fprintf(stderr, "%s:%d expected false\n", __FILE__, __LINE__);             \
            exit(1);                                                                    \
        }                                                                               \
    } while (0)

#define EXPECT_U16(EXPR, EXPECTED)                                                      \
    do {                                                                                \
        uint16_t actual_value = (uint16_t)(EXPR);                                       \
        uint16_t expected_value = (uint16_t)(EXPECTED);                                 \
        if (actual_value != expected_value) {                                           \
            fprintf(stderr, "%s:%d expected %u but got %u\n", __FILE__, __LINE__,        \
                    (unsigned)expected_value, (unsigned)actual_value);                  \
            exit(1);                                                                    \
        }                                                                               \
    } while (0)

#define EXPECT_I8(EXPR, EXPECTED)                                                       \
    do {                                                                                \
        int actual_value = (int)(EXPR);                                                 \
        int expected_value = (int)(EXPECTED);                                           \
        if (actual_value != expected_value) {                                           \
            fprintf(stderr, "%s:%d expected %d but got %d\n", __FILE__, __LINE__,        \
                    expected_value, actual_value);                                      \
            exit(1);                                                                    \
        }                                                                               \
    } while (0)
static clr_rule_profile_descriptor descriptor(uint32_t rule, uint32_t kick) {
    clr_rule_profile_descriptor value = {0};
    value.piece_set_profile_id = CLR_PIECE_SET_STANDARD_TETROMINOES;
    value.bag_profile_id = CLR_BAG_STANDARD_7_BAG;
    value.rule_profile_id = rule;
    value.kick_profile_id = kick;
    value.spawn_profile_id = CLR_SPAWN_STANDARD_10;
    value.has_verified_kick_profile = 0;
    return value;
}static void kick_transition_count_fixture(void) {
    ClearraCompactRuleProfile profile;
    clr_rule_profile_descriptor srs = descriptor(CLR_RULE_SRS, CLR_KICK_SRS_90);
    EXPECT_STATUS(clearra_rule_profile_from_descriptor(&srs, &profile), CLEARRA_RULE_OK);
    EXPECT_U16(profile.kick_table.transition_count, 56);
    EXPECT_FALSE(profile.supports_180);

    clr_rule_profile_descriptor srs_plus =
        descriptor(CLR_RULE_SRS_PLUS, CLR_KICK_SRS_PLUS_180);
    EXPECT_STATUS(clearra_rule_profile_from_descriptor(&srs_plus, &profile),
                  CLEARRA_RULE_OK);
    EXPECT_U16(profile.kick_table.transition_count, 80);
    EXPECT_TRUE(profile.supports_180);

    const ClearraCompactKickSequence *i_half = 0;
    EXPECT_STATUS(
        clearra_kick_table_sequence_for(
            &profile.kick_table,
            CLR_PIECE_I,
            CLEARRA_RULE_ROTATION_SPAWN,
            CLEARRA_RULE_ROTATION_REVERSE,
            &i_half),
        CLEARRA_RULE_OK);
    EXPECT_U16(i_half->count, 6);
    EXPECT_I8(i_half->offsets[0].dx, 0);
    EXPECT_I8(i_half->offsets[0].dy, 0);
    EXPECT_I8(i_half->offsets[5].dx, -1);
    EXPECT_I8(i_half->offsets[5].dy, 0);
}static void srs_transition_offsets_are_compact_runtime_view(void) {
    ClearraCompactRuleProfile profile;
    const ClearraCompactKickSequence *sequence = 0;
    clr_rule_profile_descriptor srs = descriptor(CLR_RULE_SRS, CLR_KICK_SRS_90);
    EXPECT_STATUS(clearra_rule_profile_from_descriptor(&srs, &profile), CLEARRA_RULE_OK);

    EXPECT_STATUS(
        clearra_kick_table_sequence_for(
            &profile.kick_table,
            CLR_PIECE_T,
            CLEARRA_RULE_ROTATION_SPAWN,
            CLEARRA_RULE_ROTATION_RIGHT,
            &sequence),
        CLEARRA_RULE_OK);
    EXPECT_U16(sequence->count, 5);
    EXPECT_I8(sequence->offsets[1].dx, -1);
    EXPECT_I8(sequence->offsets[1].dy, 0);
}static void no_kick_has_zero_offset_only_fixture(void) {
    ClearraCompactRuleProfile profile;
    const ClearraCompactKickSequence *sequence = 0;
    clr_rule_profile_descriptor no_kick = descriptor(CLR_RULE_NO_KICK, CLR_KICK_NO_KICK);
    EXPECT_STATUS(clearra_rule_profile_from_descriptor(&no_kick, &profile),
                  CLEARRA_RULE_OK);

    EXPECT_U16(profile.kick_table.transition_count, 56);
    EXPECT_FALSE(profile.supports_180);
    EXPECT_TRUE(clearra_kick_table_zero_offsets_only(&profile.kick_table));
    EXPECT_STATUS(
        clearra_kick_table_sequence_for(
            &profile.kick_table,
            CLR_PIECE_I,
            CLEARRA_RULE_ROTATION_RIGHT,
            CLEARRA_RULE_ROTATION_REVERSE,
            &sequence),
        CLEARRA_RULE_OK);
    EXPECT_U16(sequence->count, 1);
    EXPECT_I8(sequence->offsets[0].dx, 0);
    EXPECT_I8(sequence->offsets[0].dy, 0);
}static void unsupported_rule_returns_status_fixture(void) {
    ClearraCompactRuleProfile profile;
    clr_rule_profile_descriptor srs_x = descriptor(CLR_RULE_SRS_X, CLR_KICK_SRS_X);
    EXPECT_STATUS(clearra_rule_profile_from_descriptor(&srs_x, &profile),
                  CLEARRA_RULE_UNSUPPORTED_RULE);

    clr_rule_profile_descriptor imported = descriptor(CLR_RULE_SRS, CLR_KICK_IMPORTED);
    EXPECT_STATUS(clearra_rule_profile_from_descriptor(&imported, &profile),
                  CLEARRA_RULE_UNSUPPORTED_KICK_PROFILE);
}static void imported_verified_kick_profile_compiles_to_compact_descriptor_fixture(void) {
    ClearraCompactRuleProfile profile;
    const ClearraCompactKickSequence *sequence = 0;
    clr_rule_profile_descriptor imported = descriptor(CLR_RULE_SRS_X, CLR_KICK_IMPORTED);
    imported.has_verified_kick_profile = 1;
    imported.verified_supports_180 = 1;
    imported.verified_transition_count = 1;
    imported.verified_transitions[0].piece = CLR_PIECE_T;
    imported.verified_transitions[0].from_rotation = CLEARRA_RULE_ROTATION_SPAWN;
    imported.verified_transitions[0].to_rotation = CLEARRA_RULE_ROTATION_RIGHT;
    imported.verified_transitions[0].sequence.count = 2;
    imported.verified_transitions[0].sequence.offsets[0].dx = 0;
    imported.verified_transitions[0].sequence.offsets[0].dy = 0;
    imported.verified_transitions[0].sequence.offsets[1].dx = -1;
    imported.verified_transitions[0].sequence.offsets[1].dy = 0;

    EXPECT_STATUS(clearra_rule_profile_from_descriptor(&imported, &profile),
                  CLEARRA_RULE_OK);
    EXPECT_U16(profile.rule_profile_id, CLR_RULE_SRS_X);
    EXPECT_U16(profile.kick_profile_id, CLR_KICK_IMPORTED);
    EXPECT_U16(profile.kick_table.transition_count, 1);
    EXPECT_TRUE(profile.supports_180);
    EXPECT_STATUS(
        clearra_kick_table_sequence_for(
            &profile.kick_table,
            CLR_PIECE_T,
            CLEARRA_RULE_ROTATION_SPAWN,
            CLEARRA_RULE_ROTATION_RIGHT,
            &sequence),
        CLEARRA_RULE_OK);
    EXPECT_U16(sequence->count, 2);
    EXPECT_I8(sequence->offsets[1].dx, -1);
}

static void verified_srs_x_max_sequence_round_trips_fixture(void) {
    ClearraCompactRuleProfile profile;
    const ClearraCompactKickSequence *sequence = 0;
    clr_rule_profile_descriptor srs_x =
        descriptor(CLR_RULE_SRS_X, CLR_KICK_SRS_X);
    srs_x.has_verified_kick_profile = 1;
    srs_x.verified_supports_180 = 1;
    srs_x.verified_transition_count = 1;
    srs_x.verified_transitions[0].piece = CLR_PIECE_T;
    srs_x.verified_transitions[0].from_rotation =
        CLEARRA_RULE_ROTATION_SPAWN;
    srs_x.verified_transitions[0].to_rotation =
        CLEARRA_RULE_ROTATION_REVERSE;
    srs_x.verified_transitions[0].sequence.count =
        CLR_RULE_MAX_KICK_OFFSETS;
    for (uint8_t index = 0; index < CLR_RULE_MAX_KICK_OFFSETS; ++index) {
        srs_x.verified_transitions[0].sequence.offsets[index].dx =
            (int8_t)index;
        srs_x.verified_transitions[0].sequence.offsets[index].dy =
            (int8_t)(-((int8_t)index));
    }

    EXPECT_STATUS(clearra_rule_profile_from_descriptor(&srs_x, &profile),
                  CLEARRA_RULE_OK);
    EXPECT_STATUS(
        clearra_kick_table_sequence_for(
            &profile.kick_table,
            CLR_PIECE_T,
            CLEARRA_RULE_ROTATION_SPAWN,
            CLEARRA_RULE_ROTATION_REVERSE,
            &sequence),
        CLEARRA_RULE_OK);
    EXPECT_U16(sequence->count, 12);
    for (uint8_t index = 0; index < CLR_RULE_MAX_KICK_OFFSETS; ++index) {
        EXPECT_I8(sequence->offsets[index].dx, (int8_t)index);
        EXPECT_I8(sequence->offsets[index].dy, (int8_t)(-((int8_t)index)));
    }
}

static void srs_plus_capability_reported_fixture(void) {
    ClearraCompactRuleProfile profile;
    const ClearraCompactKickSequence *sequence = 0;
    const ClearraCompactKickSequence *mirrored_sequence = 0;
    clr_rule_profile_descriptor srs_plus =
        descriptor(CLR_RULE_SRS_PLUS, CLR_KICK_SRS_PLUS_180);
    EXPECT_STATUS(clearra_rule_profile_from_descriptor(&srs_plus, &profile),
                  CLEARRA_RULE_OK);

    EXPECT_TRUE(profile.srs_plus_capability_reported);
    EXPECT_TRUE(profile.kick_table.srs_plus_capability_reported);
    EXPECT_TRUE(clearra_kick_table_supports_180(&profile.kick_table));
    EXPECT_STATUS(
        clearra_kick_table_sequence_for(
            &profile.kick_table,
            CLR_PIECE_T,
            CLEARRA_RULE_ROTATION_SPAWN,
            CLEARRA_RULE_ROTATION_REVERSE,
            &sequence),
        CLEARRA_RULE_OK);
    EXPECT_U16(sequence->count, 6);
    EXPECT_I8(sequence->offsets[1].dx, 0);
    EXPECT_I8(sequence->offsets[1].dy, 1);
    EXPECT_I8(sequence->offsets[2].dx, 1);
    EXPECT_I8(sequence->offsets[2].dy, 1);

    EXPECT_STATUS(
        clearra_kick_table_sequence_for(
            &profile.kick_table,
            CLR_PIECE_I,
            CLEARRA_RULE_ROTATION_SPAWN,
            CLEARRA_RULE_ROTATION_REVERSE,
            &sequence),
        CLEARRA_RULE_OK);
    EXPECT_U16(sequence->count, 6);
    EXPECT_I8(sequence->offsets[0].dx, 0);
    EXPECT_I8(sequence->offsets[0].dy, 0);
    EXPECT_I8(sequence->offsets[1].dx, 0);
    EXPECT_I8(sequence->offsets[1].dy, 1);
    EXPECT_I8(sequence->offsets[5].dx, -1);
    EXPECT_I8(sequence->offsets[5].dy, 0);

    EXPECT_STATUS(
        clearra_kick_table_sequence_for(
            &profile.kick_table,
            CLR_PIECE_O,
            CLEARRA_RULE_ROTATION_SPAWN,
            CLEARRA_RULE_ROTATION_REVERSE,
            &sequence),
        CLEARRA_RULE_TRANSITION_NOT_FOUND);

    EXPECT_STATUS(
        clearra_kick_table_sequence_for(
            &profile.kick_table,
            CLR_PIECE_I,
            CLEARRA_RULE_ROTATION_SPAWN,
            CLEARRA_RULE_ROTATION_RIGHT,
            &sequence),
        CLEARRA_RULE_OK);
    EXPECT_STATUS(
        clearra_kick_table_sequence_for(
            &profile.kick_table,
            CLR_PIECE_I,
            CLEARRA_RULE_ROTATION_SPAWN,
            CLEARRA_RULE_ROTATION_LEFT,
            &mirrored_sequence),
        CLEARRA_RULE_OK);
    EXPECT_U16(sequence->count, 5);
    EXPECT_U16(mirrored_sequence->count, 5);
    for (uint8_t index = 0; index < sequence->count; index++) {
        EXPECT_I8(mirrored_sequence->offsets[index].dx,
                  -sequence->offsets[index].dx);
        EXPECT_I8(mirrored_sequence->offsets[index].dy,
                  sequence->offsets[index].dy);
    }
}

static void jstris_180_profile_matches_two_kick_reference_fixture(void) {
    ClearraCompactRuleProfile profile;
    const ClearraCompactKickSequence *sequence = 0;
    clr_rule_profile_descriptor jstris =
        descriptor(CLR_RULE_JSTRIS_180, CLR_KICK_JSTRIS_180);
    EXPECT_STATUS(clearra_rule_profile_from_descriptor(&jstris, &profile),
                  CLEARRA_RULE_OK);

    EXPECT_U16(profile.kick_table.transition_count, 72);
    EXPECT_TRUE(profile.supports_180);
    EXPECT_FALSE(profile.srs_plus_capability_reported);

    const uint8_t from_rotations[4] = {
        CLEARRA_RULE_ROTATION_SPAWN,
        CLEARRA_RULE_ROTATION_RIGHT,
        CLEARRA_RULE_ROTATION_REVERSE,
        CLEARRA_RULE_ROTATION_LEFT,
    };
    const uint8_t to_rotations[4] = {
        CLEARRA_RULE_ROTATION_REVERSE,
        CLEARRA_RULE_ROTATION_LEFT,
        CLEARRA_RULE_ROTATION_SPAWN,
        CLEARRA_RULE_ROTATION_RIGHT,
    };
    const int8_t expected_dx[4] = {0, 1, 0, -1};
    const int8_t expected_dy[4] = {1, 0, -1, 0};
    const uint8_t pieces[6] = {
        CLR_PIECE_I,
        CLR_PIECE_J,
        CLR_PIECE_L,
        CLR_PIECE_S,
        CLR_PIECE_T,
        CLR_PIECE_Z,
    };

    for (uint8_t piece_index = 0; piece_index < 6; piece_index++) {
        for (uint8_t transition_index = 0; transition_index < 4;
             transition_index++) {
            EXPECT_STATUS(
                clearra_kick_table_sequence_for(
                    &profile.kick_table,
                    pieces[piece_index],
                    from_rotations[transition_index],
                    to_rotations[transition_index],
                    &sequence),
                CLEARRA_RULE_OK);
            EXPECT_U16(sequence->count, 2);
            EXPECT_I8(sequence->offsets[0].dx, 0);
            EXPECT_I8(sequence->offsets[0].dy, 0);
            EXPECT_I8(sequence->offsets[1].dx, expected_dx[transition_index]);
            EXPECT_I8(sequence->offsets[1].dy, expected_dy[transition_index]);
        }
    }

    EXPECT_STATUS(
        clearra_kick_table_sequence_for(
            &profile.kick_table,
            CLR_PIECE_I,
            CLEARRA_RULE_ROTATION_SPAWN,
            CLEARRA_RULE_ROTATION_RIGHT,
            &sequence),
        CLEARRA_RULE_OK);
    EXPECT_U16(sequence->count, 5);
    EXPECT_I8(sequence->offsets[1].dx, -2);
    EXPECT_I8(sequence->offsets[1].dy, 0);

    EXPECT_STATUS(
        clearra_kick_table_sequence_for(
            &profile.kick_table,
            CLR_PIECE_O,
            CLEARRA_RULE_ROTATION_SPAWN,
            CLEARRA_RULE_ROTATION_RIGHT,
            &sequence),
        CLEARRA_RULE_TRANSITION_NOT_FOUND);
}

int main(void) {
    kick_transition_count_fixture();
    srs_transition_offsets_are_compact_runtime_view();
    no_kick_has_zero_offset_only_fixture();
    unsupported_rule_returns_status_fixture();
    imported_verified_kick_profile_compiles_to_compact_descriptor_fixture();
    verified_srs_x_max_sequence_round_trips_fixture();
    srs_plus_capability_reported_fixture();
    jstris_180_profile_matches_two_kick_reference_fixture();
    return 0;
}
