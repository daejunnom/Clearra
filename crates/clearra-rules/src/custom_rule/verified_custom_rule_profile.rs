use crate::{
    kicks::VerifiedKickTableProfile,
    line_clear::LineClearPolicy,
    profile::rule_profile::{RuleProfile, RuleProfileId},
    rotation::RotationSystem,
    spawn::SpawnProfile,
};

use super::{
    custom_rule_editor_schema::{
        CustomRuleEditorDraft, CustomRuleEditorSchema, CustomRuleSpawnRule,
    },
    custom_rule_runtime::LockReachabilityPolicy,
    custom_rule_search_capability_report::CustomRuleSearchCapabilityReport,
    custom_rule_verification_report::CustomRuleVerificationReport,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedCustomRuleProfile {
    id: String,
    label: String,
    rule_profile: RuleProfile,
    verified_kick_profile: VerifiedKickTableProfile,
    spawn_profile: SpawnProfile,
    rotation_system: RotationSystem,
    lock_reachability_policy: LockReachabilityPolicy,
    line_clear_policy: LineClearPolicy,
    can_compile_to_c_descriptor: bool,
}

impl VerifiedCustomRuleProfile {
    pub fn try_from_editor_schema(
        schema: CustomRuleEditorSchema,
    ) -> Result<Self, CustomRuleVerificationReport> {
        let report = CustomRuleVerificationReport::verify_editor_schema(&schema);
        if !report.is_verified() {
            return Err(report);
        }

        let verified_kick_profile =
            VerifiedKickTableProfile::try_new(schema.kick_table_profile.clone())
                .expect("verification report already checked the kick table profile");
        let spawn_profile = schema
            .spawn_rules()
            .first()
            .map(CustomRuleSpawnRule::spawn_profile)
            .unwrap_or(SpawnProfile::STANDARD_10);
        let can_compile_to_c_descriptor = schema.can_compile_to_c_descriptor();

        Ok(Self {
            id: schema.id,
            label: schema.label,
            rule_profile: RuleProfile::new(RuleProfileId::Custom),
            verified_kick_profile,
            spawn_profile,
            rotation_system: RotationSystem::Srs,
            lock_reachability_policy: schema.lock_reachability_mode,
            line_clear_policy: schema.line_clear_policy,
            can_compile_to_c_descriptor,
        })
    }
}
impl VerifiedCustomRuleProfile {
    pub fn try_from_editor_draft(
        draft: CustomRuleEditorDraft,
    ) -> Result<Self, CustomRuleVerificationReport> {
        Self::try_from_editor_schema(CustomRuleEditorSchema::from_editor_draft(draft))
    }
}
impl VerifiedCustomRuleProfile {
    pub fn id(&self) -> &str {
        &self.id
    }
}
impl VerifiedCustomRuleProfile {
    pub fn label(&self) -> &str {
        &self.label
    }
}
impl VerifiedCustomRuleProfile {
    pub fn rule_profile(&self) -> RuleProfile {
        self.rule_profile
    }
}
impl VerifiedCustomRuleProfile {
    pub fn verified_kick_profile(&self) -> &VerifiedKickTableProfile {
        &self.verified_kick_profile
    }
}
impl VerifiedCustomRuleProfile {
    pub fn spawn_profile(&self) -> SpawnProfile {
        self.spawn_profile
    }
}
impl VerifiedCustomRuleProfile {
    pub fn rotation_system(&self) -> RotationSystem {
        self.rotation_system
    }
}
impl VerifiedCustomRuleProfile {
    pub fn lock_reachability_policy(&self) -> LockReachabilityPolicy {
        self.lock_reachability_policy
    }
}
impl VerifiedCustomRuleProfile {
    pub fn line_clear_policy(&self) -> LineClearPolicy {
        self.line_clear_policy
    }
}
impl VerifiedCustomRuleProfile {
    pub fn can_compile_to_c_descriptor(&self) -> bool {
        self.can_compile_to_c_descriptor
    }
}
impl VerifiedCustomRuleProfile {
    pub fn search_capability_report(&self) -> CustomRuleSearchCapabilityReport {
        CustomRuleSearchCapabilityReport::from_verified_profile(self)
    }
}
