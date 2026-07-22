use clearra_app::AppResponse;

use super::result_kind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiExportPanel {
    label_i18n_key: &'static str,
    copy_available: bool,
    export_available: bool,
    default_export_kind: String,
    json_contract_keys_localized: bool,
}

impl GuiExportPanel {
    pub fn from_response(response: &AppResponse) -> Self {
        Self {
            label_i18n_key: "ui.result.export",
            copy_available: response.render_model().is_some(),
            export_available: response.render_model().is_some(),
            default_export_kind: result_kind(response),
            json_contract_keys_localized: false,
        }
    }
}
impl GuiExportPanel {
    pub const fn label_i18n_key(&self) -> &'static str {
        self.label_i18n_key
    }
}
impl GuiExportPanel {
    pub const fn copy_available(&self) -> bool {
        self.copy_available
    }
}
impl GuiExportPanel {
    pub const fn export_available(&self) -> bool {
        self.export_available
    }
}
impl GuiExportPanel {
    pub fn default_export_kind(&self) -> &str {
        &self.default_export_kind
    }
}
impl GuiExportPanel {
    pub const fn json_contract_keys_localized(&self) -> bool {
        self.json_contract_keys_localized
    }
}
