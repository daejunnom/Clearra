use clearra_validation::diagnostic::diagnostic_code::DiagnosticCode;

use crate::{disabled_reason::UiDisabledReason, i18n::LocalizedLabelSchema};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DropdownOption {
    value: String,
    label: String,
    localized_label: Option<LocalizedLabelSchema>,
    disabled_reason: Option<UiDisabledReason>,
}

impl DropdownOption {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            localized_label: None,
            disabled_reason: None,
        }
    }
}
impl DropdownOption {
    pub fn with_localized_label(mut self, localized_label: LocalizedLabelSchema) -> Self {
        self.localized_label = Some(localized_label);
        self
    }
}
impl DropdownOption {
    pub fn disabled_for(mut self, code: DiagnosticCode, reason: impl Into<String>) -> Self {
        self.disabled_reason = Some(UiDisabledReason::new(code, reason));
        self
    }
}
impl DropdownOption {
    pub fn value(&self) -> &str {
        &self.value
    }
}
impl DropdownOption {
    pub fn label(&self) -> &str {
        &self.label
    }
}
impl DropdownOption {
    pub fn localized_label(&self) -> Option<&LocalizedLabelSchema> {
        self.localized_label.as_ref()
    }
}
impl DropdownOption {
    pub fn is_disabled(&self) -> bool {
        self.disabled_reason.is_some()
    }
}
impl DropdownOption {
    pub fn disabled_reason(&self) -> Option<&UiDisabledReason> {
        self.disabled_reason.as_ref()
    }
}
