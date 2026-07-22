use std::path::{Path, PathBuf};

use clearra_app::GuiStatePersistenceContract;

use super::{
    load_settings_or_default, save_settings, LoadedSettings, SettingsError, SettingsModel,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}
impl SettingsStore {
    pub fn default_relative() -> Self {
        Self::new(Self::default_preference_path())
    }
}
impl SettingsStore {
    pub fn default_preference_path() -> &'static str {
        GuiStatePersistenceContract::preference_path()
    }
}
impl SettingsStore {
    pub fn stable_keys() -> &'static [&'static str] {
        SettingsModel::stable_json_keys()
    }
}
impl SettingsStore {
    pub fn path(&self) -> &Path {
        &self.path
    }
}
impl SettingsStore {
    pub fn load_or_default(&self) -> LoadedSettings {
        load_settings_or_default(&self.path)
    }
}
impl SettingsStore {
    pub fn save(&self, settings: &SettingsModel) -> Result<(), SettingsError> {
        save_settings(&self.path, settings)
    }
}
