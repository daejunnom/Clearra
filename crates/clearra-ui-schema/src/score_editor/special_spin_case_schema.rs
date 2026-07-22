use clearra_i18n::TranslationKey;
use clearra_scoring::spin::{SpecialSpinCaseId, SpecialSpinVerificationState};
use clearra_validation::diagnostic::diagnostic_code::DiagnosticCode;

use crate::{disabled_reason::UiDisabledReason, i18n::LocalizedLabelSchema};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpecialSpinCaseSchema {
    cases: Vec<SpecialSpinCaseOptionSchema>,
}

impl SpecialSpinCaseSchema {
    pub fn mvp2() -> Self {
        Self {
            cases: vec![
                descriptor_only_case(SpecialSpinCaseId::Fin, "Fin"),
                descriptor_only_case(SpecialSpinCaseId::Iso, "ISO"),
                descriptor_only_case(SpecialSpinCaseId::Neo, "NEO"),
            ],
        }
    }
}
impl SpecialSpinCaseSchema {
    pub fn cases(&self) -> &[SpecialSpinCaseOptionSchema] {
        &self.cases
    }
}

impl Default for SpecialSpinCaseSchema {
    fn default() -> Self {
        Self::mvp2()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpecialSpinCaseOptionSchema {
    id: SpecialSpinCaseId,
    label: &'static str,
    localized_label: LocalizedLabelSchema,
    verification_state: SpecialSpinVerificationState,
    kick_evidence_required: bool,
    disabled_reason: Option<UiDisabledReason>,
}

impl SpecialSpinCaseOptionSchema {
    pub fn id(&self) -> &SpecialSpinCaseId {
        &self.id
    }
}
impl SpecialSpinCaseOptionSchema {
    pub fn id_str(&self) -> &str {
        self.id.as_str()
    }
}
impl SpecialSpinCaseOptionSchema {
    pub fn label(&self) -> &'static str {
        self.label
    }
}
impl SpecialSpinCaseOptionSchema {
    pub fn localized_label(&self) -> &LocalizedLabelSchema {
        &self.localized_label
    }
}
impl SpecialSpinCaseOptionSchema {
    pub fn verification_state(&self) -> SpecialSpinVerificationState {
        self.verification_state
    }
}
impl SpecialSpinCaseOptionSchema {
    pub fn kick_evidence_required(&self) -> bool {
        self.kick_evidence_required
    }
}
impl SpecialSpinCaseOptionSchema {
    pub fn disabled_reason(&self) -> Option<&UiDisabledReason> {
        self.disabled_reason.as_ref()
    }
}

fn descriptor_only_case(id: SpecialSpinCaseId, label: &'static str) -> SpecialSpinCaseOptionSchema {
    let id_text = id.as_str().to_owned();
    SpecialSpinCaseOptionSchema {
        id,
        label,
        localized_label: LocalizedLabelSchema::new(
            TranslationKey::new(format!("ui.special_spin_case.{id_text}.label")),
            label,
        ),
        verification_state: SpecialSpinVerificationState::DescriptorOnly,
        kick_evidence_required: true,
        disabled_reason: Some(UiDisabledReason::new(
            DiagnosticCode::ESpinProfileUnverified,
            "verified_fixture_required",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_schema_exposes_special_spin_disabled_reason() {
        let schema = SpecialSpinCaseSchema::mvp2();

        for special_case in schema.cases() {
            assert!(matches!(
                special_case.verification_state(),
                SpecialSpinVerificationState::DescriptorOnly
            ));
            let reason = special_case
                .disabled_reason()
                .expect("descriptor-only special spin disabled reason");
            assert_eq!(reason.code(), DiagnosticCode::ESpinProfileUnverified);
            assert_eq!(reason.reason(), "verified_fixture_required");
            assert!(special_case.kick_evidence_required());
        }
    }
}
