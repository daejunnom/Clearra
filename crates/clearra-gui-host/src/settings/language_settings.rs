use clearra_i18n::LanguageId;

use super::SettingsModel;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LanguageSettings {
    language: String,
    label_i18n_key: &'static str,
}

impl LanguageSettings {
    pub fn from_model(model: &SettingsModel) -> Self {
        Self {
            language: model.language().to_owned(),
            label_i18n_key: "ui.settings.language",
        }
    }
}
impl LanguageSettings {
    pub fn language(&self) -> &str {
        &self.language
    }
}
impl LanguageSettings {
    pub fn language_id(&self) -> LanguageId {
        LanguageId::parse(&self.language).unwrap_or(LanguageId::En)
    }
}
impl LanguageSettings {
    pub const fn label_i18n_key(&self) -> &'static str {
        self.label_i18n_key
    }
}
