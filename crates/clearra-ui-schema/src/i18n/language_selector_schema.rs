use clearra_i18n::{LanguageId, LanguagePreference};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LanguageSelectorSchema {
    default_language: LanguageId,
    detected_language: Option<LanguageId>,
    selected_language: Option<LanguageId>,
    options: Vec<LanguageOptionSchema>,
}

impl LanguageSelectorSchema {
    pub fn from_preference(preference: &LanguagePreference) -> Self {
        let detected_language = preference.system_locale().and_then(LanguageId::parse);
        Self {
            default_language: LanguageId::En,
            detected_language,
            selected_language: preference.selected(),
            options: LanguageId::ALL
                .into_iter()
                .map(LanguageOptionSchema::new)
                .collect(),
        }
    }
}
impl LanguageSelectorSchema {
    pub fn mvp() -> Self {
        Self::from_preference(&LanguagePreference::default())
    }
}
impl LanguageSelectorSchema {
    pub fn default_language(&self) -> LanguageId {
        self.default_language
    }
}
impl LanguageSelectorSchema {
    pub fn detected_language(&self) -> Option<LanguageId> {
        self.detected_language
    }
}
impl LanguageSelectorSchema {
    pub fn selected_language(&self) -> Option<LanguageId> {
        self.selected_language
    }
}
impl LanguageSelectorSchema {
    pub fn resolved_language(&self) -> LanguageId {
        self.selected_language
            .or(self.detected_language)
            .unwrap_or(self.default_language)
    }
}
impl LanguageSelectorSchema {
    pub fn options(&self) -> &[LanguageOptionSchema] {
        &self.options
    }
}

impl Default for LanguageSelectorSchema {
    fn default() -> Self {
        Self::mvp()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LanguageOptionSchema {
    id: LanguageId,
    native_label: &'static str,
    english_label: &'static str,
}

impl LanguageOptionSchema {
    pub fn new(id: LanguageId) -> Self {
        Self {
            id,
            native_label: id.native_label(),
            english_label: id.english_label(),
        }
    }
}
impl LanguageOptionSchema {
    pub fn id(&self) -> LanguageId {
        self.id
    }
}
impl LanguageOptionSchema {
    pub fn native_label(&self) -> &'static str {
        self.native_label
    }
}
impl LanguageOptionSchema {
    pub fn english_label(&self) -> &'static str {
        self.english_label
    }
}

#[cfg(test)]
#[path = "language_selector_schema_tests.rs"]
mod tests;
