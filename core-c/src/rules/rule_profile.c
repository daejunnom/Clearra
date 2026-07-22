#include "rules.h"
static ClearraRuleStatus validate_common_descriptor(
    const clr_rule_profile_descriptor *descriptor) {
    if (descriptor == 0) {
        return CLEARRA_RULE_INVALID_ARGUMENT;
    }
    if (descriptor->piece_set_profile_id != CLR_PIECE_SET_STANDARD_TETROMINOES ||
        descriptor->bag_profile_id != CLR_BAG_STANDARD_7_BAG) {
        return CLEARRA_RULE_UNSUPPORTED_RULE;
    }
    if (descriptor->has_verified_kick_profile != 0u) {
        if (descriptor->kick_profile_id == CLR_KICK_CUSTOM ||
            descriptor->verified_transition_count == 0u ||
            descriptor->verified_transition_count > CLR_RULE_MAX_KICK_TRANSITIONS) {
            return CLEARRA_RULE_UNSUPPORTED_KICK_PROFILE;
        }
        return CLEARRA_RULE_OK;
    }
    if (descriptor->kick_profile_id == CLR_KICK_IMPORTED ||
        descriptor->kick_profile_id == CLR_KICK_CUSTOM) {
        return CLEARRA_RULE_UNSUPPORTED_KICK_PROFILE;
    }
    return CLEARRA_RULE_OK;
}static ClearraRuleStatus fill_verified_kick_table(
    const clr_rule_profile_descriptor *descriptor,
    ClearraCompactKickTable *out_table) {
    clearra_kick_table_clear(
        out_table,
        descriptor->kick_profile_id,
        descriptor->rule_profile_id,
        descriptor->verified_supports_180 != 0u,
        descriptor->rule_profile_id == CLR_RULE_SRS_PLUS);

    for (uint16_t index = 0u; index < descriptor->verified_transition_count; ++index) {
        const clr_kick_transition_descriptor *source =
            &descriptor->verified_transitions[index];
        if (source->sequence.count > CLEARRA_RULE_MAX_KICK_OFFSETS) {
            return CLEARRA_RULE_UNSUPPORTED_KICK_PROFILE;
        }

        ClearraCompactKickSequence sequence = {0};
        sequence.count = source->sequence.count;
        for (uint8_t offset_index = 0u; offset_index < source->sequence.count;
             ++offset_index) {
            sequence.offsets[offset_index].dx =
                source->sequence.offsets[offset_index].dx;
            sequence.offsets[offset_index].dy =
                source->sequence.offsets[offset_index].dy;
        }

        ClearraRuleStatus status = clearra_kick_table_push(
            out_table,
            source->piece,
            source->from_rotation,
            source->to_rotation,
            sequence);
        if (status != CLEARRA_RULE_OK) {
            return status;
        }
    }
    return CLEARRA_RULE_OK;
}static ClearraRuleStatus fill_kick_table(
    const clr_rule_profile_descriptor *descriptor,
    ClearraCompactKickTable *out_table) {
    if (descriptor->has_verified_kick_profile != 0u) {
        return fill_verified_kick_table(descriptor, out_table);
    }

    if (descriptor->rule_profile_id == CLR_RULE_SRS &&
        descriptor->kick_profile_id == CLR_KICK_SRS_90) {
        return clearra_srs_kick_table(out_table);
    }
    if (descriptor->rule_profile_id == CLR_RULE_SRS_PLUS &&
        descriptor->kick_profile_id == CLR_KICK_SRS_PLUS_180) {
        return clearra_srs_plus_kick_table(out_table);
    }
    if (descriptor->rule_profile_id == CLR_RULE_NO_KICK &&
        descriptor->kick_profile_id == CLR_KICK_NO_KICK) {
        return clearra_no_kick_table(out_table);
    }

    if (descriptor->rule_profile_id == CLR_RULE_SRS_PLUS ||
        descriptor->rule_profile_id == CLR_RULE_SRS ||
        descriptor->rule_profile_id == CLR_RULE_NO_KICK) {
        return CLEARRA_RULE_UNSUPPORTED_KICK_PROFILE;
    }
    return CLEARRA_RULE_UNSUPPORTED_RULE;
}ClearraRuleStatus clearra_rule_profile_from_descriptor(
    const clr_rule_profile_descriptor *descriptor,
    ClearraCompactRuleProfile *out_profile) {
    if (out_profile == 0) {
        return CLEARRA_RULE_INVALID_ARGUMENT;
    }

    ClearraRuleStatus status = validate_common_descriptor(descriptor);
    if (status != CLEARRA_RULE_OK) {
        return status;
    }

    status = fill_kick_table(descriptor, &out_profile->kick_table);
    if (status != CLEARRA_RULE_OK) {
        return status;
    }

    status = clearra_spawn_profile_from_id(
        descriptor->spawn_profile_id,
        &out_profile->spawn_profile);
    if (status != CLEARRA_RULE_OK) {
        return status;
    }

    out_profile->piece_set_profile_id = descriptor->piece_set_profile_id;
    out_profile->bag_profile_id = descriptor->bag_profile_id;
    out_profile->rule_profile_id = descriptor->rule_profile_id;
    out_profile->kick_profile_id = descriptor->kick_profile_id;
    out_profile->spawn_profile_id = descriptor->spawn_profile_id;
    out_profile->supports_180 =
        clearra_kick_table_supports_180(&out_profile->kick_table);
    out_profile->srs_plus_capability_reported =
        out_profile->kick_table.srs_plus_capability_reported;
    return CLEARRA_RULE_OK;
}