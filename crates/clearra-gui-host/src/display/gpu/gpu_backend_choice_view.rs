use clearra_app::AppResponse;

use super::super::{field_value, first_field};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiGpuBackendChoiceView {
    backend_requested: String,
    backend_selected: String,
    gpu_backend_kind: String,
}

impl GuiGpuBackendChoiceView {
    pub fn from_response(response: &AppResponse) -> Self {
        let backend_selected = first_field(response, &["backend_selected", "selected_backend"])
            .unwrap_or_else(|| "none".to_owned());
        let gpu_backend_kind = field_value(response, "gpu_backend_kind")
            .or_else(|| field_value(response, "gpu_worker_state"))
            .unwrap_or_else(|| {
                if backend_selected == "gpu" {
                    "gpu".to_owned()
                } else {
                    "not-used".to_owned()
                }
            });

        Self {
            backend_requested: first_field(response, &["backend_requested", "requested_backend"])
                .unwrap_or_else(|| "none".to_owned()),
            backend_selected,
            gpu_backend_kind,
        }
    }
}
impl GuiGpuBackendChoiceView {
    pub fn backend_requested(&self) -> &str {
        &self.backend_requested
    }
}
impl GuiGpuBackendChoiceView {
    pub fn backend_selected(&self) -> &str {
        &self.backend_selected
    }
}
impl GuiGpuBackendChoiceView {
    pub fn gpu_backend_kind(&self) -> &str {
        &self.gpu_backend_kind
    }
}
