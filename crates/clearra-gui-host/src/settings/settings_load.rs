use std::{fs, path::Path};

use clearra_validation::diagnostic::diagnostic_report::DiagnosticReport;

use super::{
    settings_error::{SettingsError, SettingsErrorCode},
    SettingsModel,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoadedSettings {
    settings: SettingsModel,
    diagnostics: DiagnosticReport,
    loaded_from_disk: bool,
}

impl LoadedSettings {
    pub fn new(
        settings: SettingsModel,
        diagnostics: DiagnosticReport,
        loaded_from_disk: bool,
    ) -> Self {
        Self {
            settings,
            diagnostics,
            loaded_from_disk,
        }
    }
}
impl LoadedSettings {
    pub const fn settings(&self) -> &SettingsModel {
        &self.settings
    }
}
impl LoadedSettings {
    pub const fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }
}
impl LoadedSettings {
    pub const fn loaded_from_disk(&self) -> bool {
        self.loaded_from_disk
    }
}

pub fn load_settings_or_default(path: &Path) -> LoadedSettings {
    match fs::read_to_string(path) {
        Ok(contents) => match serde_json::from_str::<SettingsModel>(&contents) {
            Ok(settings) => LoadedSettings::new(settings, DiagnosticReport::new(), true),
            Err(error) => {
                let settings_error =
                    SettingsError::new(SettingsErrorCode::ParseFailed, path, error.to_string());
                let mut diagnostics = DiagnosticReport::new();
                diagnostics.push(settings_error.to_diagnostic());
                LoadedSettings::new(SettingsModel::default(), diagnostics, false)
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            LoadedSettings::new(SettingsModel::default(), DiagnosticReport::new(), false)
        }
        Err(error) => {
            let settings_error =
                SettingsError::new(SettingsErrorCode::ReadFailed, path, error.to_string());
            let mut diagnostics = DiagnosticReport::new();
            diagnostics.push(settings_error.to_diagnostic());
            LoadedSettings::new(SettingsModel::default(), diagnostics, false)
        }
    }
}
