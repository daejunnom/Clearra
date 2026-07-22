#ifndef CLEARRA_CORE_C_RULES_H
#define CLEARRA_CORE_C_RULES_H

#include "clr_piece.h"
#include "clr_rules.h"

#include <stdbool.h>
#include <stdint.h>

#define CLEARRA_RULE_STANDARD_PIECE_COUNT 7u
#define CLEARRA_RULE_ROTATION_COUNT 4u
#define CLEARRA_RULE_90_TRANSITION_COUNT 8u
#define CLEARRA_RULE_180_TRANSITION_COUNT 4u
#define CLEARRA_RULE_MAX_KICK_OFFSETS 6u
#define CLEARRA_RULE_MAX_KICK_TRANSITIONS \
    (CLEARRA_RULE_STANDARD_PIECE_COUNT *                              \
     (CLEARRA_RULE_90_TRANSITION_COUNT + CLEARRA_RULE_180_TRANSITION_COUNT))
typedef enum ClearraRuleStatus {
    CLEARRA_RULE_OK = 0,
    CLEARRA_RULE_INVALID_ARGUMENT = 1,
    CLEARRA_RULE_UNSUPPORTED_RULE = 2,
    CLEARRA_RULE_UNSUPPORTED_KICK_PROFILE = 3,
    CLEARRA_RULE_UNSUPPORTED_SPAWN_PROFILE = 4,
    CLEARRA_RULE_TABLE_CAPACITY_EXCEEDED = 5,
    CLEARRA_RULE_TRANSITION_NOT_FOUND = 6
} ClearraRuleStatus;typedef enum ClearraRuleRotation {
    CLEARRA_RULE_ROTATION_SPAWN = 0,
    CLEARRA_RULE_ROTATION_RIGHT = 1,
    CLEARRA_RULE_ROTATION_REVERSE = 2,
    CLEARRA_RULE_ROTATION_LEFT = 3
} ClearraRuleRotation;typedef struct ClearraCompactKickOffset {
    int8_t dx;
    int8_t dy;
} ClearraCompactKickOffset;typedef struct ClearraCompactKickSequence {
    ClearraCompactKickOffset offsets[CLEARRA_RULE_MAX_KICK_OFFSETS];
    uint8_t count;
} ClearraCompactKickSequence;typedef struct ClearraCompactKickTransition {
    uint8_t piece;
    uint8_t from_rotation;
    uint8_t to_rotation;
    ClearraCompactKickSequence sequence;
} ClearraCompactKickTransition;typedef struct ClearraCompactKickTable {
    uint32_t kick_profile_id;
    uint32_t source_rule_profile_id;
    bool supports_180;
    bool srs_plus_capability_reported;
    uint16_t transition_count;
    ClearraCompactKickTransition transitions[CLEARRA_RULE_MAX_KICK_TRANSITIONS];
} ClearraCompactKickTable;typedef struct ClearraCompactSpawnProfile {
    uint32_t spawn_profile_id;
    uint8_t board_width;
    uint8_t hidden_rows;
    int8_t spawn_x[CLR_PIECE_L + 1u];
} ClearraCompactSpawnProfile;typedef struct ClearraCompactRuleProfile {
    uint32_t piece_set_profile_id;
    uint32_t bag_profile_id;
    uint32_t rule_profile_id;
    uint32_t kick_profile_id;
    uint32_t spawn_profile_id;
    bool supports_180;
    bool srs_plus_capability_reported;
    ClearraCompactKickTable kick_table;
    ClearraCompactSpawnProfile spawn_profile;
} ClearraCompactRuleProfile;bool clearra_rule_rotation_is_valid(uint8_t rotation);
bool clearra_rule_transition_is_180(uint8_t from_rotation, uint8_t to_rotation);
void clearra_kick_table_clear(
    ClearraCompactKickTable *table,
    uint32_t kick_profile_id,
    uint32_t source_rule_profile_id,
    bool supports_180,
    bool srs_plus_capability_reported);
ClearraRuleStatus clearra_kick_table_push(
    ClearraCompactKickTable *table,
    uint8_t piece,
    uint8_t from_rotation,
    uint8_t to_rotation,
    ClearraCompactKickSequence sequence);
ClearraRuleStatus clearra_kick_table_sequence_for(
    const ClearraCompactKickTable *table,
    uint8_t piece,
    uint8_t from_rotation,
    uint8_t to_rotation,
    const ClearraCompactKickSequence **out_sequence);
bool clearra_kick_table_supports_180(const ClearraCompactKickTable *table);
bool clearra_kick_table_zero_offsets_only(const ClearraCompactKickTable *table);ClearraCompactKickSequence clearra_no_kick_sequence(void);
ClearraRuleStatus clearra_no_kick_table(ClearraCompactKickTable *out_table);ClearraRuleStatus clearra_srs_kick_table(ClearraCompactKickTable *out_table);
ClearraRuleStatus clearra_srs_plus_kick_table(ClearraCompactKickTable *out_table);ClearraRuleStatus clearra_spawn_profile_from_id(
    uint32_t spawn_profile_id,
    ClearraCompactSpawnProfile *out_profile);ClearraRuleStatus clearra_rule_profile_from_descriptor(
    const clr_rule_profile_descriptor *descriptor,
    ClearraCompactRuleProfile *out_profile);
#endif
