use clearra_scoring::profile::{ScoreProfile, ScoreProfileRegistry};

use crate::dropdown::DropdownOption;

use super::{
    score_expectation_schema::ScoreExpectationSchema,
    score_profile_editor_fields::{
        attack_fields, b2b_fields, combo_fields, profile_fields, score_fields, spin_fields,
    },
    score_profile_result_contract_fields::score_result_contract_fields,
    scoring_field_schema::ScoringFieldSchema,
    special_spin_case_schema::SpecialSpinCaseSchema,
    spin_classifier_schema::SpinClassifierSchema,
    spin_target_schema::SpinTargetSchema,
};

pub use super::{
    score_profile_editor_fields::default_score_policy_markers,
    score_profile_import_export_schema::{
        score_profile_json_export_adapter_marker, score_profile_json_import_adapter_marker,
        ScoreProfileImportExportSchema,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoreProfileEditorSchema {
    enabled: bool,
    profiles: Vec<DropdownOption>,
    import_export: ScoreProfileImportExportSchema,
    fields: Vec<ScoringFieldSchema>,
    score_fields: Vec<ScoringFieldSchema>,
    attack_fields: Vec<ScoringFieldSchema>,
    spin_fields: Vec<ScoringFieldSchema>,
    combo_fields: Vec<ScoringFieldSchema>,
    b2b_fields: Vec<ScoringFieldSchema>,
    spin_target_schema: SpinTargetSchema,
    spin_classifier_schema: SpinClassifierSchema,
    special_spin_case_schema: SpecialSpinCaseSchema,
    score_expectation_schema: ScoreExpectationSchema,
    result_contract_fields: Vec<String>,
}

impl ScoreProfileEditorSchema {
    pub fn unsupported_mvp() -> Self {
        Self::mvp2()
    }
}
impl ScoreProfileEditorSchema {
    pub fn mvp2() -> Self {
        let registry = ScoreProfileRegistry::builtins();
        Self {
            enabled: true,
            profiles: registry
                .profiles()
                .iter()
                .map(score_profile_option)
                .collect(),
            import_export: ScoreProfileImportExportSchema::json_supported(),
            fields: profile_fields(),
            score_fields: score_fields(),
            attack_fields: attack_fields(),
            spin_fields: spin_fields(),
            combo_fields: combo_fields(),
            b2b_fields: b2b_fields(),
            spin_target_schema: SpinTargetSchema::mvp2(),
            spin_classifier_schema: SpinClassifierSchema::mvp2(),
            special_spin_case_schema: SpecialSpinCaseSchema::mvp2(),
            score_expectation_schema: ScoreExpectationSchema::mvp2(),
            result_contract_fields: score_result_contract_fields(),
        }
    }
}
impl ScoreProfileEditorSchema {
    pub fn enabled(&self) -> bool {
        self.enabled
    }
}
impl ScoreProfileEditorSchema {
    pub fn profiles(&self) -> &[DropdownOption] {
        &self.profiles
    }
}
impl ScoreProfileEditorSchema {
    pub fn import_export(&self) -> &ScoreProfileImportExportSchema {
        &self.import_export
    }
}
impl ScoreProfileEditorSchema {
    pub fn fields(&self) -> &[ScoringFieldSchema] {
        &self.fields
    }
}
impl ScoreProfileEditorSchema {
    pub fn score_fields(&self) -> &[ScoringFieldSchema] {
        &self.score_fields
    }
}
impl ScoreProfileEditorSchema {
    pub fn attack_fields(&self) -> &[ScoringFieldSchema] {
        &self.attack_fields
    }
}
impl ScoreProfileEditorSchema {
    pub fn spin_fields(&self) -> &[ScoringFieldSchema] {
        &self.spin_fields
    }
}
impl ScoreProfileEditorSchema {
    pub fn combo_fields(&self) -> &[ScoringFieldSchema] {
        &self.combo_fields
    }
}
impl ScoreProfileEditorSchema {
    pub fn b2b_fields(&self) -> &[ScoringFieldSchema] {
        &self.b2b_fields
    }
}
impl ScoreProfileEditorSchema {
    pub fn spin_target_schema(&self) -> &SpinTargetSchema {
        &self.spin_target_schema
    }
}
impl ScoreProfileEditorSchema {
    pub fn spin_classifier_schema(&self) -> &SpinClassifierSchema {
        &self.spin_classifier_schema
    }
}
impl ScoreProfileEditorSchema {
    pub fn special_spin_case_schema(&self) -> &SpecialSpinCaseSchema {
        &self.special_spin_case_schema
    }
}
impl ScoreProfileEditorSchema {
    pub fn score_expectation_schema(&self) -> &ScoreExpectationSchema {
        &self.score_expectation_schema
    }
}
impl ScoreProfileEditorSchema {
    pub fn result_contract_fields(&self) -> &[String] {
        &self.result_contract_fields
    }
}

impl Default for ScoreProfileEditorSchema {
    fn default() -> Self {
        Self::mvp2()
    }
}

fn score_profile_option(profile: &ScoreProfile) -> DropdownOption {
    DropdownOption::new(profile.id(), profile.display_name())
}

#[cfg(test)]
#[path = "score_profile_editor_schema_tests.rs"]
mod tests;
