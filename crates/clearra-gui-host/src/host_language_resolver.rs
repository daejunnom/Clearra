use clearra_i18n::{LanguageId, LanguagePreference, LanguageResolver};

pub struct GuiHostLanguageResolver;

impl GuiHostLanguageResolver {
    pub const AUTO_LANGUAGE: &'static str = "auto";
}
impl GuiHostLanguageResolver {
    pub fn default_language() -> LanguageId {
        Self::resolve_with_sources(Some(Self::AUTO_LANGUAGE), None, None, None)
    }
}
impl GuiHostLanguageResolver {
    pub fn resolve_startup_language(selected_language: &str) -> LanguageId {
        Self::resolve_with_detected_sources(Some(selected_language), None)
    }
}
impl GuiHostLanguageResolver {
    pub fn resolve_with_detected_sources(
        user_selected: Option<&str>,
        stored_preference: Option<&str>,
    ) -> LanguageId {
        if let Some(language) = Self::parse_concrete_language(user_selected) {
            return language;
        }
        if let Some(language) = Self::parse_concrete_language(stored_preference) {
            return language;
        }
        if let Some(language) = std::env::var("CLEARRA_GUI_LANG")
            .ok()
            .and_then(|value| Self::parse_concrete_language(Some(&value)))
        {
            return language;
        }

        LanguageResolver::resolve_from_selected(None)
    }
}
impl GuiHostLanguageResolver {
    pub fn resolve_with_sources(
        user_selected: Option<&str>,
        stored_preference: Option<&str>,
        env_or_cli_language: Option<&str>,
        os_locale: Option<&str>,
    ) -> LanguageId {
        if let Some(language) = Self::parse_concrete_language(user_selected) {
            return language;
        }
        if let Some(language) = Self::parse_concrete_language(stored_preference) {
            return language;
        }
        if let Some(language) = Self::parse_concrete_language(env_or_cli_language) {
            return language;
        }

        LanguagePreference::new(None, os_locale).resolve()
    }
}
impl GuiHostLanguageResolver {
    fn parse_concrete_language(value: Option<&str>) -> Option<LanguageId> {
        let value = value?.trim();
        if value.is_empty() || value.eq_ignore_ascii_case(Self::AUTO_LANGUAGE) {
            return None;
        }
        LanguageId::parse(value)
    }
}
