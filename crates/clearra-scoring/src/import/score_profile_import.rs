mod accuracy_parser {
    use serde_json::Value;

    use crate::profile::{ScoringAccuracyLevel, BASIC_APPROXIMATION_REASON};

    use super::ScoreProfileImportError;

    pub(super) fn parse_accuracy_level(
        value: Option<&str>,
    ) -> Result<ScoringAccuracyLevel, ScoreProfileImportError> {
        match value {
            Some(value) => {
                let Some(level) = ScoringAccuracyLevel::parse(value) else {
                    return Err(ScoreProfileImportError::UnsupportedAccuracyLevel(
                        value.to_owned(),
                    ));
                };
                if level == ScoringAccuracyLevel::ProfileSpecificExact {
                    return Err(ScoreProfileImportError::UnsupportedAccuracyLevel(
                        value.to_owned(),
                    ));
                }
                Ok(level)
            }
            None => Ok(ScoringAccuracyLevel::BasicApproximation),
        }
    }

    pub(super) fn reject_profile_specific_exact(
        value: Option<&Value>,
    ) -> Result<(), ScoreProfileImportError> {
        let Some(value) = value else {
            return Ok(());
        };
        match value.as_bool() {
            Some(false) => Ok(()),
            Some(true) => Err(ScoreProfileImportError::UnsupportedAccuracyLevel(
                "profile_specific_exact=true".to_owned(),
            )),
            None => Err(ScoreProfileImportError::UnsupportedAccuracyLevel(
                "profile_specific_exact must be boolean".to_owned(),
            )),
        }
    }

    pub(super) fn import_accuracy_reason(reason: &str) -> &'static str {
        if reason == BASIC_APPROXIMATION_REASON {
            BASIC_APPROXIMATION_REASON
        } else {
            "imported score profile declares a basic approximation model"
        }
    }
}
mod attack_model_parser {
    use crate::{model::AttackModelRegistry, profile::AttackModelId};

    use super::ScoreProfileImportError;

    pub(super) fn parse_attack_model(
        value: Option<&str>,
    ) -> Result<AttackModelId, ScoreProfileImportError> {
        match value {
            Some(value) => AttackModelRegistry::parse(value)
                .map(|descriptor| descriptor.id())
                .ok_or_else(|| ScoreProfileImportError::UnknownAttackModel(value.to_owned())),
            None => Ok(AttackModelId::Disabled),
        }
    }
}
mod b2b_policy_parser {
    use serde_json::Value;

    use crate::profile::B2BPolicy;

    use super::{
        numeric_fields::{bool_field, u32_field, u64_field},
        ScoreProfileImportError,
    };

    pub(super) fn parse_b2b_policy(
        value: Option<&Value>,
    ) -> Result<B2BPolicy, ScoreProfileImportError> {
        let Some(value) = value else {
            return Ok(B2BPolicy::DISABLED);
        };
        let object = value.as_object().ok_or_else(|| {
            ScoreProfileImportError::InvalidB2BSetting("b2b must be an object".to_owned())
        })?;
        reject_b2b_fields(object)?;
        let enabled = bool_field(object, "enabled", false)
            .map_err(ScoreProfileImportError::InvalidB2BSetting)?;
        let score_bonus = u64_field(object, "score_bonus", 0)
            .map_err(ScoreProfileImportError::InvalidB2BSetting)?;
        let attack_bonus = u32_field(object, "attack_bonus", 0)
            .map_err(ScoreProfileImportError::InvalidB2BSetting)?;
        if !enabled && (score_bonus > 0 || attack_bonus > 0) {
            return Err(ScoreProfileImportError::InvalidB2BSetting(
                "disabled B2B policy cannot carry bonuses".to_owned(),
            ));
        }
        Ok(if enabled {
            B2BPolicy::standard(score_bonus, attack_bonus)
        } else {
            B2BPolicy::DISABLED
        })
    }

    fn reject_b2b_fields(
        object: &serde_json::Map<String, Value>,
    ) -> Result<(), ScoreProfileImportError> {
        for key in object.keys() {
            if !["enabled", "score_bonus", "attack_bonus"].contains(&key.as_str()) {
                return Err(ScoreProfileImportError::InvalidB2BSetting(format!(
                    "unknown B2B field '{key}'"
                )));
            }
        }
        Ok(())
    }
}
mod combo_policy_parser {
    use serde_json::Value;

    use crate::profile::ComboPolicy;

    use super::{
        numeric_fields::{bool_field, u32_field, u64_field},
        ScoreProfileImportError,
    };

    pub(super) fn parse_combo_policy(
        value: Option<&Value>,
    ) -> Result<ComboPolicy, ScoreProfileImportError> {
        let Some(value) = value else {
            return Ok(ComboPolicy::DISABLED);
        };
        let object = value.as_object().ok_or_else(|| {
            ScoreProfileImportError::InvalidComboSetting("combo must be an object".to_owned())
        })?;
        reject_combo_fields(object)?;
        let enabled = bool_field(object, "enabled", false)
            .map_err(ScoreProfileImportError::InvalidComboSetting)?;
        let score_bonus = u64_field(object, "score_bonus_per_combo", 0)
            .map_err(ScoreProfileImportError::InvalidComboSetting)?;
        let attack_bonus = u32_field(object, "attack_bonus_per_combo", 0)
            .map_err(ScoreProfileImportError::InvalidComboSetting)?;
        if !enabled && (score_bonus > 0 || attack_bonus > 0) {
            return Err(ScoreProfileImportError::InvalidComboSetting(
                "disabled combo policy cannot carry bonuses".to_owned(),
            ));
        }
        Ok(if enabled {
            ComboPolicy::linear(score_bonus, attack_bonus)
        } else {
            ComboPolicy::DISABLED
        })
    }

    fn reject_combo_fields(
        object: &serde_json::Map<String, Value>,
    ) -> Result<(), ScoreProfileImportError> {
        for key in object.keys() {
            if !["enabled", "score_bonus_per_combo", "attack_bonus_per_combo"]
                .contains(&key.as_str())
            {
                return Err(ScoreProfileImportError::InvalidComboSetting(format!(
                    "unknown combo field '{key}'"
                )));
            }
        }
        Ok(())
    }
}
mod drop_score_policy_parser {
    use crate::profile::DropScorePolicy;

    use super::ScoreProfileImportError;

    pub(super) fn parse_drop_score_policy(
        value: Option<&str>,
    ) -> Result<DropScorePolicy, ScoreProfileImportError> {
        match value {
            Some(value) => DropScorePolicy::parse(value)
                .ok_or_else(|| ScoreProfileImportError::UnsupportedPolicySetting(value.to_owned())),
            None => Ok(DropScorePolicy::Disabled),
        }
    }
}
mod error {
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub enum ScoreProfileImportError {
        InvalidJson,
        MissingField(&'static str),
        UnknownScoringField(String),
        UnknownScoreModel(String),
        UnknownAttackModel(String),
        UnsupportedSpinRule(String),
        UnsupportedAccuracyLevel(String),
        InvalidComboSetting(String),
        InvalidB2BSetting(String),
        UnsupportedPolicySetting(String),
    }

    impl ScoreProfileImportError {
        pub fn code(&self) -> &'static str {
            match self {
                Self::InvalidJson => "invalid_json",
                Self::MissingField(_) => "missing_field",
                Self::UnknownScoringField(_) => "unknown_scoring_field",
                Self::UnknownScoreModel(_) => "unknown_score_model",
                Self::UnknownAttackModel(_) => "unknown_attack_model",
                Self::UnsupportedSpinRule(_) => "unsupported_spin_rule",
                Self::UnsupportedAccuracyLevel(_) => "unsupported_accuracy_level",
                Self::InvalidComboSetting(_) => "invalid_combo_setting",
                Self::InvalidB2BSetting(_) => "invalid_b2b_setting",
                Self::UnsupportedPolicySetting(_) => "unsupported_policy_setting",
            }
        }
    }
}
mod importer {
    use serde_json::Value;

    use crate::profile::{ScoreProfile, BASIC_APPROXIMATION_REASON};

    use super::{
        accuracy_parser::{
            import_accuracy_reason, parse_accuracy_level, reject_profile_specific_exact,
        },
        attack_model_parser::parse_attack_model,
        b2b_policy_parser::parse_b2b_policy,
        combo_policy_parser::parse_combo_policy,
        drop_score_policy_parser::parse_drop_score_policy,
        json_fields::{optional_string_field, reject_unknown_fields, string_field},
        level_policy_parser::parse_level_policy,
        pc_bonus_policy_parser::parse_pc_bonus_policy,
        score_model_parser::parse_score_model,
        spin_award_policy_parser::parse_spin_award_policy,
        spin_rule_parser::parse_spin_rule,
        trace_requirement_parser::parse_trace_requirement,
        ScoreProfileImportError,
    };

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct ScoreProfileImport;

    impl ScoreProfileImport {
        pub fn from_json(input: &str) -> Result<ScoreProfile, ScoreProfileImportError> {
            let value: Value =
                serde_json::from_str(input).map_err(|_| ScoreProfileImportError::InvalidJson)?;
            let object = value
                .as_object()
                .ok_or(ScoreProfileImportError::InvalidJson)?;
            reject_unknown_fields(
                object.keys().map(String::as_str),
                &[
                    "id",
                    "display_name",
                    "score_model",
                    "attack_model",
                    "spin_rule",
                    "accuracy_level",
                    "profile_specific_exact",
                    "accuracy_reason",
                    "spin_award_policy",
                    "drop_score_policy",
                    "level_policy",
                    "pc_bonus_policy",
                    "trace_requirement",
                    "combo",
                    "b2b",
                ],
            )?;

            let id = string_field(object, "id")?;
            let display_name = string_field(object, "display_name")?;
            let score_model = parse_score_model(optional_string_field(object, "score_model")?)?;
            let attack_model = parse_attack_model(optional_string_field(object, "attack_model")?)?;
            let spin_rule = parse_spin_rule(optional_string_field(object, "spin_rule")?)?;
            let accuracy_level =
                parse_accuracy_level(optional_string_field(object, "accuracy_level")?)?;
            reject_profile_specific_exact(object.get("profile_specific_exact"))?;
            let accuracy_reason = optional_string_field(object, "accuracy_reason")?
                .unwrap_or(BASIC_APPROXIMATION_REASON);
            let spin_award_policy =
                parse_spin_award_policy(optional_string_field(object, "spin_award_policy")?)?;
            let drop_score_policy =
                parse_drop_score_policy(optional_string_field(object, "drop_score_policy")?)?;
            let level_policy = parse_level_policy(optional_string_field(object, "level_policy")?)?;
            let pc_bonus_policy =
                parse_pc_bonus_policy(optional_string_field(object, "pc_bonus_policy")?)?;
            let trace_requirement =
                parse_trace_requirement(optional_string_field(object, "trace_requirement")?)?;
            let combo_policy = parse_combo_policy(object.get("combo"))?;
            let b2b_policy = parse_b2b_policy(object.get("b2b"))?;

            let mut profile = ScoreProfile::new(id, display_name)
                .with_score_model(score_model)
                .with_attack_model(attack_model)
                .with_spin_rule(spin_rule)
                .with_accuracy(accuracy_level, import_accuracy_reason(accuracy_reason))
                .with_spin_award_policy(spin_award_policy)
                .with_drop_score_policy(drop_score_policy)
                .with_level_policy(level_policy)
                .with_pc_bonus_policy(pc_bonus_policy)
                .with_combo_policy(combo_policy)
                .with_b2b_policy(b2b_policy);
            if let Some(trace_requirement) = trace_requirement {
                profile = profile.with_trace_requirement(trace_requirement);
            }
            Ok(profile)
        }
    }
}
mod json_fields {
    use serde_json::{Map, Value};

    use super::ScoreProfileImportError;

    pub(super) fn reject_unknown_fields<'a>(
        keys: impl IntoIterator<Item = &'a str>,
        allowed: &[&str],
    ) -> Result<(), ScoreProfileImportError> {
        for key in keys {
            if !allowed.contains(&key) {
                return Err(ScoreProfileImportError::UnknownScoringField(key.to_owned()));
            }
        }
        Ok(())
    }

    pub(super) fn string_field<'a>(
        object: &'a Map<String, Value>,
        key: &'static str,
    ) -> Result<&'a str, ScoreProfileImportError> {
        object
            .get(key)
            .and_then(Value::as_str)
            .ok_or(ScoreProfileImportError::MissingField(key))
    }

    pub(super) fn optional_string_field<'a>(
        object: &'a Map<String, Value>,
        key: &'static str,
    ) -> Result<Option<&'a str>, ScoreProfileImportError> {
        object
            .get(key)
            .map(|value| {
                value
                    .as_str()
                    .ok_or(ScoreProfileImportError::MissingField(key))
            })
            .transpose()
    }
}
mod level_policy_parser {
    use crate::profile::LevelPolicy;

    use super::ScoreProfileImportError;

    pub(super) fn parse_level_policy(
        value: Option<&str>,
    ) -> Result<LevelPolicy, ScoreProfileImportError> {
        match value {
            Some(value) => LevelPolicy::parse(value)
                .ok_or_else(|| ScoreProfileImportError::UnsupportedPolicySetting(value.to_owned())),
            None => Ok(LevelPolicy::Disabled),
        }
    }
}
mod numeric_fields {
    use serde_json::{Map, Value};

    pub(super) fn bool_field(
        object: &Map<String, Value>,
        key: &str,
        default: bool,
    ) -> Result<bool, String> {
        match object.get(key) {
            Some(value) => value
                .as_bool()
                .ok_or_else(|| format!("{key} must be a boolean")),
            None => Ok(default),
        }
    }

    pub(super) fn u64_field(
        object: &Map<String, Value>,
        key: &str,
        default: u64,
    ) -> Result<u64, String> {
        match object.get(key) {
            Some(value) => value.as_u64().ok_or_else(|| format!("{key} must be u64")),
            None => Ok(default),
        }
    }

    pub(super) fn u32_field(
        object: &Map<String, Value>,
        key: &str,
        default: u32,
    ) -> Result<u32, String> {
        let value = u64_field(object, key, u64::from(default))?;
        u32::try_from(value).map_err(|_| format!("{key} exceeds u32"))
    }
}
mod pc_bonus_policy_parser {
    use crate::profile::PcBonusPolicy;

    use super::ScoreProfileImportError;

    pub(super) fn parse_pc_bonus_policy(
        value: Option<&str>,
    ) -> Result<PcBonusPolicy, ScoreProfileImportError> {
        match value {
            Some(value) => PcBonusPolicy::parse(value)
                .ok_or_else(|| ScoreProfileImportError::UnsupportedPolicySetting(value.to_owned())),
            None => Ok(PcBonusPolicy::Disabled),
        }
    }
}
mod score_model_parser {
    use crate::{model::ScoreModelRegistry, profile::ScoreModelId};

    use super::ScoreProfileImportError;

    pub(super) fn parse_score_model(
        value: Option<&str>,
    ) -> Result<ScoreModelId, ScoreProfileImportError> {
        match value {
            Some(value) => ScoreModelRegistry::parse(value)
                .map(|descriptor| descriptor.id())
                .ok_or_else(|| ScoreProfileImportError::UnknownScoreModel(value.to_owned())),
            None => Ok(ScoreModelId::Disabled),
        }
    }
}
mod spin_award_policy_parser {
    use crate::profile::SpinAwardPolicy;

    use super::ScoreProfileImportError;

    pub(super) fn parse_spin_award_policy(
        value: Option<&str>,
    ) -> Result<SpinAwardPolicy, ScoreProfileImportError> {
        match value {
            Some(value) => SpinAwardPolicy::parse(value)
                .ok_or_else(|| ScoreProfileImportError::UnsupportedPolicySetting(value.to_owned())),
            None => Ok(SpinAwardPolicy::Disabled),
        }
    }
}
mod spin_rule_parser {
    use crate::profile::SpinRuleId;

    use super::ScoreProfileImportError;

    pub(super) fn parse_spin_rule(
        value: Option<&str>,
    ) -> Result<SpinRuleId, ScoreProfileImportError> {
        match value {
            Some(value) => SpinRuleId::parse(value)
                .ok_or_else(|| ScoreProfileImportError::UnsupportedSpinRule(value.to_owned())),
            None => Ok(SpinRuleId::Disabled),
        }
    }
}
mod trace_requirement_parser {
    use crate::profile::TraceRequirement;

    use super::ScoreProfileImportError;

    pub(super) fn parse_trace_requirement(
        value: Option<&str>,
    ) -> Result<Option<TraceRequirement>, ScoreProfileImportError> {
        match value {
            Some(value) => TraceRequirement::parse(value)
                .map(Some)
                .ok_or_else(|| ScoreProfileImportError::UnsupportedPolicySetting(value.to_owned())),
            None => Ok(None),
        }
    }
}

pub use error::ScoreProfileImportError;
pub use importer::ScoreProfileImport;

#[cfg(test)]
use crate::profile::{AttackModelId, ScoreModelId, ScoringAccuracyLevel, SpinRuleId};

#[cfg(test)]
#[path = "score_profile_import_tests.rs"]
mod tests;
