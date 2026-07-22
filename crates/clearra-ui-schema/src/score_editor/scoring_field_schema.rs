use clearra_validation::diagnostic::diagnostic_code::DiagnosticCode;

use crate::disabled_reason::UiDisabledReason;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoringFieldSchema {
    key: String,
    label: String,
    field_type: ScoringFieldType,
    required: bool,
    options: Vec<String>,
    disabled_reason: Option<UiDisabledReason>,
}

impl ScoringFieldSchema {
    pub fn enabled_field(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            field_type: ScoringFieldType::Text,
            required: false,
            options: Vec::new(),
            disabled_reason: None,
        }
    }
}
impl ScoringFieldSchema {
    pub fn typed_field(
        key: impl Into<String>,
        label: impl Into<String>,
        field_type: ScoringFieldType,
        required: bool,
        options: Vec<String>,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            field_type,
            required,
            options,
            disabled_reason: None,
        }
    }
}
impl ScoringFieldSchema {
    pub fn disabled_for(
        key: impl Into<String>,
        label: impl Into<String>,
        code: DiagnosticCode,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
            field_type: ScoringFieldType::Text,
            required: false,
            options: Vec::new(),
            disabled_reason: Some(UiDisabledReason::new(code, reason)),
        }
    }
}
impl ScoringFieldSchema {
    pub fn key(&self) -> &str {
        &self.key
    }
}
impl ScoringFieldSchema {
    pub fn label(&self) -> &str {
        &self.label
    }
}
impl ScoringFieldSchema {
    pub fn field_type(&self) -> ScoringFieldType {
        self.field_type
    }
}
impl ScoringFieldSchema {
    pub fn is_required(&self) -> bool {
        self.required
    }
}
impl ScoringFieldSchema {
    pub fn options(&self) -> &[String] {
        &self.options
    }
}
impl ScoringFieldSchema {
    pub fn enabled(&self) -> bool {
        self.disabled_reason.is_none()
    }
}
impl ScoringFieldSchema {
    pub fn disabled_reason(&self) -> Option<&UiDisabledReason> {
        self.disabled_reason.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScoringFieldType {
    Text,
    Select,
    Number,
    Toggle,
}
