use clearra_app::AppResponse;

use super::{field_value, first_field};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiBackendReportPanel {
    label_i18n_key: &'static str,
    backend_requested: String,
    backend_selected: String,
    gpu_trust_state: String,
    fallback_reason: String,
    cpu_confirm_required: bool,
}

impl GuiBackendReportPanel {
    pub fn from_response(response: &AppResponse) -> Self {
        Self {
            label_i18n_key: "ui.result.backend",
            backend_requested: first_field(response, &["backend_requested", "requested_backend"])
                .unwrap_or_else(|| "none".to_owned()),
            backend_selected: first_field(response, &["backend_selected", "selected_backend"])
                .unwrap_or_else(|| "none".to_owned()),
            gpu_trust_state: field_value(response, "gpu_trust_state")
                .unwrap_or_else(|| "not-used".to_owned()),
            fallback_reason: first_field(
                response,
                &["backend_fallback_reason", "gpu_worker_fallback_reason"],
            )
            .unwrap_or_else(|| "none".to_owned()),
            cpu_confirm_required: field_value(response, "gpu_cpu_confirm_required")
                .or_else(|| field_value(response, "cpu_confirm_required"))
                .and_then(|value| value.parse().ok())
                .unwrap_or(false),
        }
    }
}
impl GuiBackendReportPanel {
    pub const fn label_i18n_key(&self) -> &'static str {
        self.label_i18n_key
    }
}
impl GuiBackendReportPanel {
    pub fn backend_requested(&self) -> &str {
        &self.backend_requested
    }
}
impl GuiBackendReportPanel {
    pub fn backend_selected(&self) -> &str {
        &self.backend_selected
    }
}
impl GuiBackendReportPanel {
    pub fn gpu_trust_state(&self) -> &str {
        &self.gpu_trust_state
    }
}
impl GuiBackendReportPanel {
    pub fn fallback_reason(&self) -> &str {
        &self.fallback_reason
    }
}
impl GuiBackendReportPanel {
    pub const fn cpu_confirm_required(&self) -> bool {
        self.cpu_confirm_required
    }
}
