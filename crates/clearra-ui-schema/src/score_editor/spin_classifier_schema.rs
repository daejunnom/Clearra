use clearra_i18n::TranslationKey;
use clearra_scoring::{profile::SpinProfileRegistry, spin::SpinClassifierCapability};
use clearra_validation::diagnostic::diagnostic_code::DiagnosticCode;

use crate::{disabled_reason::UiDisabledReason, i18n::LocalizedLabelSchema};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinClassifierSchema {
    options: Vec<SpinClassifierOptionSchema>,
}

impl SpinClassifierSchema {
    pub fn mvp2() -> Self {
        Self {
            options: SpinProfileRegistry::builtins()
                .profiles()
                .iter()
                .map(|profile| {
                    SpinClassifierOptionSchema::enabled(
                        profile.id().as_str(),
                        profile.id().display_name(),
                        SpinClassifierCapability::SourcePinnedExact,
                        true,
                        true,
                    )
                })
                .collect(),
        }
    }
}
impl SpinClassifierSchema {
    pub fn options(&self) -> &[SpinClassifierOptionSchema] {
        &self.options
    }
}

impl Default for SpinClassifierSchema {
    fn default() -> Self {
        Self::mvp2()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpinClassifierOptionSchema {
    id: &'static str,
    label: &'static str,
    localized_label: LocalizedLabelSchema,
    capability: SpinClassifierCapability,
    supports_exact: bool,
    requires_kick_evidence: bool,
    disabled_reason: Option<UiDisabledReason>,
}

impl SpinClassifierOptionSchema {
    pub fn enabled(
        id: &'static str,
        label: &'static str,
        capability: SpinClassifierCapability,
        supports_exact: bool,
        requires_kick_evidence: bool,
    ) -> Self {
        Self {
            id,
            label,
            localized_label: LocalizedLabelSchema::new(
                TranslationKey::new(format!("ui.spin_classifier.{id}.label")),
                label,
            ),
            capability,
            supports_exact,
            requires_kick_evidence,
            disabled_reason: None,
        }
    }
}
impl SpinClassifierOptionSchema {
    pub fn disabled_for(mut self, code: DiagnosticCode, reason: impl Into<String>) -> Self {
        self.disabled_reason = Some(UiDisabledReason::new(code, reason));
        self
    }
}
impl SpinClassifierOptionSchema {
    pub fn id(&self) -> &'static str {
        self.id
    }
}
impl SpinClassifierOptionSchema {
    pub fn label(&self) -> &'static str {
        self.label
    }
}
impl SpinClassifierOptionSchema {
    pub fn localized_label(&self) -> &LocalizedLabelSchema {
        &self.localized_label
    }
}
impl SpinClassifierOptionSchema {
    pub fn capability(&self) -> SpinClassifierCapability {
        self.capability
    }
}
impl SpinClassifierOptionSchema {
    pub fn supports_exact(&self) -> bool {
        self.supports_exact
    }
}
impl SpinClassifierOptionSchema {
    pub fn requires_kick_evidence(&self) -> bool {
        self.requires_kick_evidence
    }
}
impl SpinClassifierOptionSchema {
    pub fn disabled_reason(&self) -> Option<&UiDisabledReason> {
        self.disabled_reason.as_ref()
    }
}
