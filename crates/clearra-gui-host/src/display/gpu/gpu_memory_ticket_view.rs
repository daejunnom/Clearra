use clearra_app::AppResponse;

use super::super::first_field;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiGpuMemoryTicketView {
    memory_ticket_id: String,
    fence_epoch: String,
    issued: bool,
}

impl GuiGpuMemoryTicketView {
    pub fn from_response(response: &AppResponse) -> Self {
        let memory_ticket_id = first_field(
            response,
            &[
                "gpu_memory_ticket_id",
                "memory_ticket_id",
                "gpu_worker_memory_ticket_id",
            ],
        )
        .unwrap_or_else(|| "not-issued".to_owned());
        let fence_epoch = first_field(
            response,
            &["gpu_fence_epoch", "fence_epoch", "gpu_worker_fence_epoch"],
        )
        .unwrap_or_else(|| "none".to_owned());
        let issued = !matches!(
            memory_ticket_id.as_str(),
            "not-issued" | "none" | "null" | "0"
        );

        Self {
            memory_ticket_id,
            fence_epoch,
            issued,
        }
    }
}
impl GuiGpuMemoryTicketView {
    pub fn memory_ticket_id(&self) -> &str {
        &self.memory_ticket_id
    }
}
impl GuiGpuMemoryTicketView {
    pub fn fence_epoch(&self) -> &str {
        &self.fence_epoch
    }
}
impl GuiGpuMemoryTicketView {
    pub const fn issued(&self) -> bool {
        self.issued
    }
}
