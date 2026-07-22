use clearra_rules::{
    kicks::{KickTableEntry, VerifiedKickTableProfile},
    profile::{
        rule_capability::RuleCapability,
        rule_profile::{RuleProfile, RuleProfileId},
    },
};

use crate::problem::{
    CKickOffsetDescriptor, CKickSequenceDescriptor, CKickTransitionDescriptor,
    CRuleProfileDescriptor, FfiProblemError, C_RULE_MAX_KICK_OFFSETS, C_RULE_MAX_KICK_TRANSITIONS,
};

use super::kick_table_identity_mapper::{
    kick_profile_code, piece_code, rotation_code, rule_profile_code,
};

pub(crate) fn compile_verified_profile(
    mut descriptor: CRuleProfileDescriptor,
    rule: RuleProfile,
    verified_profile: &VerifiedKickTableProfile,
) -> Result<CRuleProfileDescriptor, FfiProblemError> {
    let profile = verified_profile.profile();
    if profile.source_rule() != rule.id() {
        return Err(FfiProblemError::VerifiedKickProfileRuleMismatch {
            rule_profile_id: descriptor.rule_profile_id,
            source_rule_profile_id: rule_profile_code(profile.source_rule()),
        });
    }
    if matches!(rule.id(), RuleProfileId::Asc | RuleProfileId::Ars) {
        return Err(FfiProblemError::SpawnAwareRuleProfileRejected {
            rule_profile_id: descriptor.rule_profile_id,
        });
    }
    if RuleCapability::from_rule(rule).supports_180() && !profile.supports_180() {
        return Err(FfiProblemError::VerifiedKickProfileMissingRequired180 {
            rule_profile_id: descriptor.rule_profile_id,
        });
    }
    if profile.entries().len() > C_RULE_MAX_KICK_TRANSITIONS {
        return Err(FfiProblemError::KickTransitionCountTooLarge {
            transition_count: profile.entries().len(),
        });
    }

    descriptor.kick_profile_id = kick_profile_code(profile.id());
    descriptor.has_verified_kick_profile = 1;
    descriptor.verified_supports_180 = profile.supports_180() as u8;
    descriptor.verified_transition_count = profile.entries().len() as u16;

    for (index, entry) in profile.entries().iter().enumerate() {
        descriptor.verified_transitions[index] = compact_transition(entry)?;
    }

    Ok(descriptor)
}

fn compact_transition(
    entry: &KickTableEntry,
) -> Result<CKickTransitionDescriptor, FfiProblemError> {
    let offsets = entry.sequence().offsets();
    if offsets.len() > C_RULE_MAX_KICK_OFFSETS {
        return Err(FfiProblemError::KickOffsetSequenceTooLong {
            offset_count: offsets.len(),
        });
    }

    let mut sequence = CKickSequenceDescriptor {
        count: offsets.len() as u8,
        ..CKickSequenceDescriptor::default()
    };
    for (index, offset) in offsets.iter().enumerate() {
        sequence.offsets[index] = CKickOffsetDescriptor {
            dx: offset.dx(),
            dy: offset.dy(),
        };
    }

    let transition = entry.transition();
    Ok(CKickTransitionDescriptor {
        piece: piece_code(transition.piece()),
        from_rotation: rotation_code(transition.from()),
        to_rotation: rotation_code(transition.to()),
        reserved: 0,
        sequence,
    })
}
