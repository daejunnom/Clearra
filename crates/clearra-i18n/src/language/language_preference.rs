use super::{language_id::normalize_language, LanguageId};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LanguagePreference {
    selected: Option<LanguageId>,
    system_locale: Option<String>,
}

impl LanguagePreference {
    pub fn new(selected: Option<LanguageId>, system_locale: Option<impl Into<String>>) -> Self {
        Self {
            selected,
            system_locale: system_locale.map(Into::into),
        }
    }
}
impl LanguagePreference {
    pub fn selected(&self) -> Option<LanguageId> {
        self.selected
    }
}
impl LanguagePreference {
    pub fn system_locale(&self) -> Option<&str> {
        self.system_locale.as_deref()
    }
}
impl LanguagePreference {
    pub fn with_selected(mut self, selected: Option<LanguageId>) -> Self {
        self.selected = selected;
        self
    }
}
impl LanguagePreference {
    pub fn with_system_locale(mut self, system_locale: Option<impl Into<String>>) -> Self {
        self.system_locale = system_locale.map(Into::into);
        self
    }
}
impl LanguagePreference {
    pub fn resolve(&self) -> LanguageId {
        if let Some(selected) = self.selected {
            return selected;
        }

        match self.system_locale.as_deref() {
            Some(locale) if normalize_language(locale).starts_with("ko") => LanguageId::Ko,
            _ => LanguageId::En,
        }
    }
}

#[cfg(test)]
#[path = "language_preference_tests.rs"]
mod tests;
