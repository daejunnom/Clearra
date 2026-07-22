#include "rules.h"

ClearraRuleStatus clearra_spawn_profile_from_id(
    uint32_t spawn_profile_id,
    ClearraCompactSpawnProfile *out_profile) {
    if (out_profile == 0) {
        return CLEARRA_RULE_INVALID_ARGUMENT;
    }
    if (spawn_profile_id != CLR_SPAWN_STANDARD_10) {
        return CLEARRA_RULE_UNSUPPORTED_SPAWN_PROFILE;
    }

    out_profile->spawn_profile_id = spawn_profile_id;
    out_profile->board_width = 10;
    out_profile->hidden_rows = 2;
    for (uint8_t piece = 0; piece <= CLR_PIECE_L; piece++) {
        out_profile->spawn_x[piece] = 3;
    }
    out_profile->spawn_x[CLR_PIECE_O] = 4;
    return CLEARRA_RULE_OK;
}
