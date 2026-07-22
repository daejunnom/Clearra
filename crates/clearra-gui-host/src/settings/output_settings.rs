use crate::GuiOutputFormat;

use super::SettingsModel;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OutputSettings {
    default_output_format: String,
    label_i18n_key: &'static str,
}

impl OutputSettings {
    pub fn from_model(model: &SettingsModel) -> Self {
        Self {
            default_output_format: model.default_output_format().to_owned(),
            label_i18n_key: "ui.settings.output",
        }
    }
}
impl OutputSettings {
    pub fn default_output_format(&self) -> &str {
        &self.default_output_format
    }
}
impl OutputSettings {
    pub fn output_format(&self) -> GuiOutputFormat {
        match self.default_output_format.as_str() {
            "json" => GuiOutputFormat::Json,
            "fumen-like" => GuiOutputFormat::FumenLike,
            "render" => GuiOutputFormat::Render,
            _ => GuiOutputFormat::Text,
        }
    }
}
impl OutputSettings {
    pub const fn label_i18n_key(&self) -> &'static str {
        self.label_i18n_key
    }
}
