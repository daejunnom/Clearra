pub mod custom_rule_editor_contract;
mod custom_rule_editor_schema;
mod custom_rule_runtime;
mod custom_rule_search_capability_report;
mod custom_rule_verification_report;
mod verified_custom_rule_profile;

pub use custom_rule_editor_contract::{
    CustomRuleBoardBackend, CustomRuleEditorDraft, CustomRuleEditorSchema,
    CustomRulePieceSpecificOverride, CustomRuleProfileVerificationError,
    CustomRuleProfileVerificationReport, CustomRuleRuntimeFeature,
    CustomRuleSearchCapabilityReport, CustomRuleSpawnRule, CustomRuleVerificationIssue,
    CustomRuleVerificationReport, LockReachabilityPolicy, VerifiedCustomRuleProfile,
};

fn single_piece_id(value: &str) -> Option<clearra_core_domain::piece::piece_kind::PieceKind> {
    let mut chars = value.chars();
    let piece = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    clearra_core_domain::piece::piece_kind::PieceKind::from_ascii(piece).ok()
}
