use clearra_scoring::{
    profile::{AllSpinScoreMapping, DropScorePolicy, ScoringAccuracyLevel, SpinAwardPolicy},
    spin::TraceCompleteness,
};

use crate::diagnostic::diagnostic_report::DiagnosticReport;

use super::{
    score_profile_object_field_validator::validate_unknown_fields,
    score_profile_object_policy_validator::{
        validate_all_mini_classifier_contract, validate_all_spin_classifier_contract,
        validate_default_all_spin_policy, validate_drop_score_trace_contract,
        validate_profile_specific_exact_contract,
    },
    score_profile_object_registry_validator::{
        validate_model_registry_ids, validate_spin_classifier_registry_id,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoreProfileObjectDescriptor {
    profile_id: String,
    score_model_id: String,
    attack_model_id: String,
    spin_classifier_id: String,
    spin_award_policy: SpinAwardPolicy,
    all_spin_score_mapping: AllSpinScoreMapping,
    all_mini_policy_enabled: bool,
    drop_score_policy: DropScorePolicy,
    accuracy_level: ScoringAccuracyLevel,
    exact_score_table_pinned: bool,
    exact_spin_classifier_available: bool,
    drop_score_basis_sufficient: bool,
    profile_specific_fixtures_pass: bool,
    trace_completeness: TraceCompleteness,
    unknown_fields: Vec<String>,
}

impl ScoreProfileObjectDescriptor {
    pub fn new(profile_id: impl Into<String>) -> Self {
        Self {
            profile_id: profile_id.into(),
            score_model_id: "disabled".to_owned(),
            attack_model_id: "disabled".to_owned(),
            spin_classifier_id: "disabled".to_owned(),
            spin_award_policy: SpinAwardPolicy::Disabled,
            all_spin_score_mapping: AllSpinScoreMapping::Disabled,
            all_mini_policy_enabled: false,
            drop_score_policy: DropScorePolicy::Disabled,
            accuracy_level: ScoringAccuracyLevel::BasicApproximation,
            exact_score_table_pinned: false,
            exact_spin_classifier_available: false,
            drop_score_basis_sufficient: false,
            profile_specific_fixtures_pass: false,
            trace_completeness: TraceCompleteness::Incomplete,
            unknown_fields: Vec::new(),
        }
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn with_score_model_id(mut self, score_model_id: impl Into<String>) -> Self {
        self.score_model_id = score_model_id.into();
        self
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn with_attack_model_id(mut self, attack_model_id: impl Into<String>) -> Self {
        self.attack_model_id = attack_model_id.into();
        self
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn with_spin_classifier_id(mut self, spin_classifier_id: impl Into<String>) -> Self {
        self.spin_classifier_id = spin_classifier_id.into();
        self
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn with_spin_award_policy(mut self, spin_award_policy: SpinAwardPolicy) -> Self {
        self.spin_award_policy = spin_award_policy;
        self
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn with_all_spin_score_mapping(
        mut self,
        all_spin_score_mapping: AllSpinScoreMapping,
    ) -> Self {
        self.all_spin_score_mapping = all_spin_score_mapping;
        self
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn with_all_mini_policy_enabled(mut self, enabled: bool) -> Self {
        self.all_mini_policy_enabled = enabled;
        self
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn with_drop_score_policy(mut self, drop_score_policy: DropScorePolicy) -> Self {
        self.drop_score_policy = drop_score_policy;
        self
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn with_accuracy_level(mut self, accuracy_level: ScoringAccuracyLevel) -> Self {
        self.accuracy_level = accuracy_level;
        self
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn with_exact_score_table_pinned(mut self, pinned: bool) -> Self {
        self.exact_score_table_pinned = pinned;
        self
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn with_exact_spin_classifier_available(mut self, available: bool) -> Self {
        self.exact_spin_classifier_available = available;
        self
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn with_drop_score_basis_sufficient(mut self, sufficient: bool) -> Self {
        self.drop_score_basis_sufficient = sufficient;
        self
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn with_profile_specific_fixtures_pass(mut self, pass: bool) -> Self {
        self.profile_specific_fixtures_pass = pass;
        self
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn with_trace_completeness(mut self, trace_completeness: TraceCompleteness) -> Self {
        self.trace_completeness = trace_completeness;
        self
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn with_unknown_field(mut self, field: impl Into<String>) -> Self {
        self.unknown_fields.push(field.into());
        self
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn profile_id(&self) -> &str {
        &self.profile_id
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn score_model_id(&self) -> &str {
        &self.score_model_id
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn attack_model_id(&self) -> &str {
        &self.attack_model_id
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn spin_classifier_id(&self) -> &str {
        &self.spin_classifier_id
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn spin_award_policy(&self) -> SpinAwardPolicy {
        self.spin_award_policy
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn all_spin_score_mapping(&self) -> AllSpinScoreMapping {
        self.all_spin_score_mapping
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn all_mini_policy_enabled(&self) -> bool {
        self.all_mini_policy_enabled
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn drop_score_policy(&self) -> DropScorePolicy {
        self.drop_score_policy
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn accuracy_level(&self) -> ScoringAccuracyLevel {
        self.accuracy_level
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn exact_score_table_pinned(&self) -> bool {
        self.exact_score_table_pinned
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn exact_spin_classifier_available(&self) -> bool {
        self.exact_spin_classifier_available
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn drop_score_basis_sufficient(&self) -> bool {
        self.drop_score_basis_sufficient
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn profile_specific_fixtures_pass(&self) -> bool {
        self.profile_specific_fixtures_pass
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn trace_completeness(&self) -> TraceCompleteness {
        self.trace_completeness
    }
}
impl ScoreProfileObjectDescriptor {
    pub fn unknown_fields(&self) -> &[String] {
        &self.unknown_fields
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ScoreProfileObjectValidator;

impl ScoreProfileObjectValidator {
    pub fn validate(object: &ScoreProfileObjectDescriptor) -> DiagnosticReport {
        let mut report = DiagnosticReport::new();
        validate_unknown_fields(object, &mut report);
        validate_model_registry_ids(object, &mut report);
        validate_spin_classifier_registry_id(object, &mut report);
        validate_profile_specific_exact_contract(object, &mut report);
        validate_drop_score_trace_contract(object, &mut report);
        validate_default_all_spin_policy(object, &mut report);
        validate_all_spin_classifier_contract(object, &mut report);
        validate_all_mini_classifier_contract(object, &mut report);
        report
    }
}

pub fn validate_score_profile_object(object: &ScoreProfileObjectDescriptor) -> DiagnosticReport {
    ScoreProfileObjectValidator::validate(object)
}

#[cfg(test)]
#[path = "score_profile_object_validator_tests.rs"]
mod tests;
