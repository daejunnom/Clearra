use serde::{Deserialize, Serialize};

use crate::{GuiBackendChoice, GuiOutputFormat, GuiUserPreferences};

pub const SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SettingsTheme {
    System,
    Light,
    Dark,
}

impl SettingsTheme {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SettingsModel {
    schema_version: u32,
    language: String,
    backend: String,
    recent_problem_preset: String,
    workers: u16,
    allow_backend_fallback: bool,
    deterministic: bool,
    default_output_format: String,
    last_opened_fixture_dir: Option<String>,
    theme: SettingsTheme,
}

impl SettingsModel {
    pub fn new() -> Self {
        Self::default()
    }
}
impl SettingsModel {
    pub fn from_preferences(preferences: &GuiUserPreferences) -> Self {
        Self {
            language: preferences.language().to_owned(),
            backend: preferences.backend().to_owned(),
            recent_problem_preset: preferences.recent_problem_preset().to_owned(),
            ..Self::default()
        }
    }
}
impl SettingsModel {
    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = language.into();
        self
    }
}
impl SettingsModel {
    pub fn with_backend(mut self, backend: GuiBackendChoice) -> Self {
        self.backend = backend.as_str().to_owned();
        self
    }
}
impl SettingsModel {
    pub const fn with_workers(mut self, workers: u16) -> Self {
        self.workers = workers;
        self
    }
}
impl SettingsModel {
    pub const fn with_allow_backend_fallback(mut self, allow_backend_fallback: bool) -> Self {
        self.allow_backend_fallback = allow_backend_fallback;
        self
    }
}
impl SettingsModel {
    pub const fn with_deterministic(mut self, deterministic: bool) -> Self {
        self.deterministic = deterministic;
        self
    }
}
impl SettingsModel {
    pub fn with_default_output_format(mut self, default_output_format: GuiOutputFormat) -> Self {
        self.default_output_format = default_output_format.as_str().to_owned();
        self
    }
}
impl SettingsModel {
    pub fn with_last_opened_fixture_dir(
        mut self,
        last_opened_fixture_dir: impl Into<String>,
    ) -> Self {
        self.last_opened_fixture_dir = Some(last_opened_fixture_dir.into());
        self
    }
}
impl SettingsModel {
    pub fn with_theme(mut self, theme: SettingsTheme) -> Self {
        self.theme = theme;
        self
    }
}
impl SettingsModel {
    pub const fn schema_version(&self) -> u32 {
        self.schema_version
    }
}
impl SettingsModel {
    pub fn language(&self) -> &str {
        &self.language
    }
}
impl SettingsModel {
    pub fn backend(&self) -> &str {
        &self.backend
    }
}
impl SettingsModel {
    pub fn recent_problem_preset(&self) -> &str {
        &self.recent_problem_preset
    }
}
impl SettingsModel {
    pub const fn workers(&self) -> u16 {
        self.workers
    }
}
impl SettingsModel {
    pub const fn allow_backend_fallback(&self) -> bool {
        self.allow_backend_fallback
    }
}
impl SettingsModel {
    pub const fn deterministic(&self) -> bool {
        self.deterministic
    }
}
impl SettingsModel {
    pub fn default_output_format(&self) -> &str {
        &self.default_output_format
    }
}
impl SettingsModel {
    pub fn last_opened_fixture_dir(&self) -> Option<&str> {
        self.last_opened_fixture_dir.as_deref()
    }
}
impl SettingsModel {
    pub const fn theme(&self) -> &SettingsTheme {
        &self.theme
    }
}
impl SettingsModel {
    pub fn stable_json_keys() -> &'static [&'static str] {
        &[
            "schema_version",
            "language",
            "backend",
            "recent_problem_preset",
            "workers",
            "allow_backend_fallback",
            "deterministic",
            "default_output_format",
            "last_opened_fixture_dir",
            "theme",
        ]
    }
}

impl Default for SettingsModel {
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            language: "en".to_owned(),
            backend: GuiBackendChoice::Auto.as_str().to_owned(),
            recent_problem_preset: "opening-pc".to_owned(),
            workers: 1,
            allow_backend_fallback: true,
            deterministic: true,
            default_output_format: GuiOutputFormat::Text.as_str().to_owned(),
            last_opened_fixture_dir: None,
            theme: SettingsTheme::System,
        }
    }
}
