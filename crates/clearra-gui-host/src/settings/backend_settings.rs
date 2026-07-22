use crate::GuiBackendChoice;

use super::SettingsModel;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendSettings {
    backend: String,
    workers: u16,
    allow_backend_fallback: bool,
    deterministic: bool,
    label_i18n_key: &'static str,
}

impl BackendSettings {
    pub fn from_model(model: &SettingsModel) -> Self {
        Self {
            backend: model.backend().to_owned(),
            workers: model.workers(),
            allow_backend_fallback: model.allow_backend_fallback(),
            deterministic: model.deterministic(),
            label_i18n_key: "ui.settings.backend",
        }
    }
}
impl BackendSettings {
    pub fn backend(&self) -> &str {
        &self.backend
    }
}
impl BackendSettings {
    pub fn backend_choice(&self) -> GuiBackendChoice {
        GuiBackendChoice::parse(&self.backend).unwrap_or_default()
    }
}
impl BackendSettings {
    pub const fn workers(&self) -> u16 {
        self.workers
    }
}
impl BackendSettings {
    pub const fn allow_backend_fallback(&self) -> bool {
        self.allow_backend_fallback
    }
}
impl BackendSettings {
    pub const fn deterministic(&self) -> bool {
        self.deterministic
    }
}
impl BackendSettings {
    pub const fn label_i18n_key(&self) -> &'static str {
        self.label_i18n_key
    }
}
