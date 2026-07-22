use std::{fs, path::Path};

use super::{
    settings_error::{SettingsError, SettingsErrorCode},
    SettingsModel,
};

pub fn save_settings(path: &Path, settings: &SettingsModel) -> Result<(), SettingsError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            SettingsError::new(SettingsErrorCode::WriteFailed, path, error.to_string())
        })?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|error| {
        SettingsError::new(SettingsErrorCode::WriteFailed, path, error.to_string())
    })?;
    fs::write(path, json).map_err(|error| {
        SettingsError::new(SettingsErrorCode::WriteFailed, path, error.to_string())
    })
}
