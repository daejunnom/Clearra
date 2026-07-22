use clearra_app::AppResponse;

use super::field_value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiMemoryReportPanel {
    label_i18n_key: &'static str,
    memory_leak_clean: bool,
    memory_pressure_level: String,
    memory_ticket_id: String,
    fence_epoch: String,
    pending_release_queue: String,
}

impl GuiMemoryReportPanel {
    pub fn from_response(response: &AppResponse) -> Self {
        Self {
            label_i18n_key: "ui.result.memory",
            memory_leak_clean: field_value(response, "memory_leak_report_clean")
                .and_then(|value| value.parse().ok())
                .unwrap_or(false),
            memory_pressure_level: field_value(response, "memory_pressure_level")
                .unwrap_or_else(|| "not_measured".to_owned()),
            memory_ticket_id: field_value(response, "gpu_memory_ticket_id")
                .or_else(|| field_value(response, "memory_ticket_id"))
                .unwrap_or_else(|| "not-issued".to_owned()),
            fence_epoch: field_value(response, "gpu_fence_epoch")
                .or_else(|| field_value(response, "fence_epoch"))
                .unwrap_or_else(|| "none".to_owned()),
            pending_release_queue: field_value(response, "pending_release_queue")
                .unwrap_or_else(|| "not_measured".to_owned()),
        }
    }
}
impl GuiMemoryReportPanel {
    pub const fn label_i18n_key(&self) -> &'static str {
        self.label_i18n_key
    }
}
impl GuiMemoryReportPanel {
    pub const fn memory_leak_clean(&self) -> bool {
        self.memory_leak_clean
    }
}
impl GuiMemoryReportPanel {
    pub fn memory_pressure_level(&self) -> &str {
        &self.memory_pressure_level
    }
}
impl GuiMemoryReportPanel {
    pub fn memory_ticket_id(&self) -> &str {
        &self.memory_ticket_id
    }
}
impl GuiMemoryReportPanel {
    pub fn fence_epoch(&self) -> &str {
        &self.fence_epoch
    }
}
impl GuiMemoryReportPanel {
    pub fn pending_release_queue(&self) -> &str {
        &self.pending_release_queue
    }
}
