#ifndef CLR_RULES_H
#define CLR_RULES_H

#include <stdint.h>

#define CLR_PIECE_SET_STANDARD_TETROMINOES 1u
#define CLR_BAG_STANDARD_7_BAG 1u

#define CLR_RULE_SRS_PLUS 1u
#define CLR_RULE_SRS 2u
#define CLR_RULE_SRS_X 3u
#define CLR_RULE_ASC 4u
#define CLR_RULE_ARS 5u
#define CLR_RULE_NO_KICK 6u
#define CLR_RULE_JSTRIS_180 7u
#define CLR_RULE_CUSTOM 255u

#define CLR_KICK_SRS_90 1u
#define CLR_KICK_NO_KICK 2u
#define CLR_KICK_SRS_PLUS_180 3u
#define CLR_KICK_SRS_X 4u
#define CLR_KICK_ASC 5u
#define CLR_KICK_ARS 6u
#define CLR_KICK_IMPORTED 7u
#define CLR_KICK_JSTRIS_180 8u
#define CLR_KICK_CUSTOM 255u

#define CLR_SPAWN_STANDARD_10 1u
#define CLR_SPAWN_ARIKA 2u
#define CLR_SPAWN_CUSTOM 255u

#define CLR_RULE_MAX_KICK_OFFSETS 12u
#define CLR_RULE_MAX_KICK_TRANSITIONS 84u
typedef struct clr_kick_offset_descriptor {
    int8_t dx;
    int8_t dy;
} clr_kick_offset_descriptor;typedef struct clr_kick_sequence_descriptor {
    clr_kick_offset_descriptor offsets[CLR_RULE_MAX_KICK_OFFSETS];
    uint8_t count;
    uint8_t reserved[3];
} clr_kick_sequence_descriptor;typedef struct clr_kick_transition_descriptor {
    uint8_t piece;
    uint8_t from_rotation;
    uint8_t to_rotation;
    uint8_t reserved;
    clr_kick_sequence_descriptor sequence;
} clr_kick_transition_descriptor;typedef struct clr_rule_profile_descriptor {
    uint32_t piece_set_profile_id;
    uint32_t bag_profile_id;
    uint32_t rule_profile_id;
    uint32_t kick_profile_id;
    uint32_t spawn_profile_id;
    uint8_t has_verified_kick_profile;
    uint8_t verified_supports_180;
    uint16_t verified_transition_count;
    clr_kick_transition_descriptor verified_transitions[CLR_RULE_MAX_KICK_TRANSITIONS];
} clr_rule_profile_descriptor;
#endif
