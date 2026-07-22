use clearra_i18n::LanguageId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiUserPreferences {
    language: String,
    backend: String,
    recent_problem_preset: String,
}

impl GuiUserPreferences {
    pub fn new(
        language: impl Into<String>,
        backend: impl Into<String>,
        recent_problem_preset: impl Into<String>,
    ) -> Self {
        Self {
            language: language.into(),
            backend: backend.into(),
            recent_problem_preset: recent_problem_preset.into(),
        }
    }
}
impl GuiUserPreferences {
    pub fn language(&self) -> &str {
        &self.language
    }
}
impl GuiUserPreferences {
    pub fn language_id(&self) -> LanguageId {
        LanguageId::parse(&self.language).unwrap_or(LanguageId::En)
    }
}
impl GuiUserPreferences {
    pub fn backend(&self) -> &str {
        &self.backend
    }
}
impl GuiUserPreferences {
    pub fn recent_problem_preset(&self) -> &str {
        &self.recent_problem_preset
    }
}

impl Default for GuiUserPreferences {
    fn default() -> Self {
        Self::new("en", "auto", "opening-pc")
    }
}
