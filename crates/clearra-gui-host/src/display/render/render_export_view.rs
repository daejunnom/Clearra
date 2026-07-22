use clearra_app::AppResponse;

use super::RenderPreviewView;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderExportView {
    label_i18n_key: &'static str,
    export_available: bool,
    export_format: &'static str,
    disabled_reason: String,
}

impl RenderExportView {
    pub fn from_response(response: &AppResponse) -> Self {
        let preview = RenderPreviewView::from_response(response);
        let export_available = preview.preview_available();
        let disabled_reason = if export_available {
            "none".to_owned()
        } else {
            preview.preview_status().to_owned()
        };

        Self {
            label_i18n_key: "ui.result.render.export",
            export_available,
            export_format: "png",
            disabled_reason,
        }
    }
}
impl RenderExportView {
    pub const fn label_i18n_key(&self) -> &'static str {
        self.label_i18n_key
    }
}
impl RenderExportView {
    pub const fn export_available(&self) -> bool {
        self.export_available
    }
}
impl RenderExportView {
    pub const fn export_format(&self) -> &'static str {
        self.export_format
    }
}
impl RenderExportView {
    pub fn disabled_reason(&self) -> &str {
        &self.disabled_reason
    }
}
