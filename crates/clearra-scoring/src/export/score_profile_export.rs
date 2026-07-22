use serde_json::json;

use crate::profile::score_profile::ScoreProfile;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScoreProfileExport;

impl ScoreProfileExport {
    pub fn to_json(profile: &ScoreProfile) -> Result<String, ScoreProfileExportError> {
        let combo = profile.combo_policy();
        let b2b = profile.b2b_policy();
        let raw = json!({
            "id": profile.id(),
            "display_name": profile.display_name(),
            "score_model": profile.score_model().as_str(),
            "attack_model": profile.attack_model().as_str(),
            "spin_rule": profile.spin_rule().as_str(),
            "spin_award_policy": profile.spin_award_policy().as_str(),
            "drop_score_policy": profile.drop_score_policy().as_str(),
            "level_policy": profile.level_policy().as_str(),
            "pc_bonus_policy": profile.pc_bonus_policy().as_str(),
            "trace_requirement": profile.trace_requirement().as_str(),
            "accuracy_level": profile.accuracy_level().as_str(),
            "profile_specific_exact": profile.profile_specific_exact(),
            "accuracy_reason": profile.accuracy_reason(),
            "combo": {
                "enabled": combo.enabled(),
                "score_bonus_per_combo": combo.score_bonus_per_combo(),
                "attack_bonus_per_combo": combo.attack_bonus_per_combo()
            },
            "b2b": {
                "enabled": b2b.enabled(),
                "score_bonus": b2b.score_bonus(),
                "attack_bonus": b2b.attack_bonus()
            }
        });

        serde_json::to_string_pretty(&raw).map_err(|_| ScoreProfileExportError::InvalidProfile)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScoreProfileExportError {
    InvalidProfile,
}

#[cfg(test)]
mod tests {
    use crate::{
        export::score_profile_export::ScoreProfileExport,
        import::score_profile_import::ScoreProfileImport,
        profile::{AttackModelId, B2BPolicy, ComboPolicy, ScoreModelId, ScoreProfile, SpinRuleId},
    };

    #[test]
    fn score_profile_export_roundtrips_through_import_contract() {
        let profile = ScoreProfile::new("roundtrip", "Roundtrip")
            .with_score_model(ScoreModelId::Guideline)
            .with_attack_model(AttackModelId::Guideline)
            .with_spin_rule(SpinRuleId::TSpinCornerBased)
            .with_combo_policy(ComboPolicy::linear(25, 1))
            .with_b2b_policy(B2BPolicy::standard(100, 1));

        let json = ScoreProfileExport::to_json(&profile).expect("export");
        let reparsed = ScoreProfileImport::from_json(&json).expect("import");

        assert_eq!(reparsed, profile);
        assert!(json.contains("\"accuracy_level\": \"basic-approximation\""));
        assert!(json.contains("\"profile_specific_exact\": false"));
        assert!(json.contains("\"accuracy_reason\""));
    }
}
