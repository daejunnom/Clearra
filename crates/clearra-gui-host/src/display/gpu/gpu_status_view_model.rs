use clearra_app::AppResponse;

use super::super::{bool_field, field_value, first_field};
use super::{
    GuiGpuBackendChoiceView, GuiGpuBackpressureView, GuiGpuFallbackReasonView,
    GuiGpuMemoryTicketView, GuiGpuTrustStateView,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiGpuStatusViewModel {
    label_i18n_key: &'static str,
    backend_choice: GuiGpuBackendChoiceView,
    gpu_status: String,
    trust_state: GuiGpuTrustStateView,
    cpu_confirm_required: bool,
    can_source_exact_probability: bool,
    fallback_reason: GuiGpuFallbackReasonView,
    memory_ticket: GuiGpuMemoryTicketView,
    backpressure: GuiGpuBackpressureView,
}

impl GuiGpuStatusViewModel {
    pub fn from_response(response: &AppResponse) -> Self {
        let trust_state = GuiGpuTrustStateView::from_response(response);
        let fallback_reason = GuiGpuFallbackReasonView::from_response(response);
        let gpu_status =
            first_field(response, &["gpu_worker_state", "gpu_status"]).unwrap_or_else(|| {
                if fallback_reason.fallback_visible() {
                    "fallback".to_owned()
                } else if fallback_reason.unavailable_visible() {
                    "unavailable".to_owned()
                } else {
                    "not-used".to_owned()
                }
            });
        let cpu_confirm_required = bool_field(response, "cpu_confirm_required")
            || bool_field(response, "gpu_cpu_confirm_required");
        let can_source_exact_probability =
            field_value(response, "gpu_can_source_exact_probability")
                .and_then(|value| value.parse().ok())
                .unwrap_or_else(|| trust_state.can_source_exact_probability())
                && trust_state.exact_badge();

        Self {
            label_i18n_key: "ui.result.gpu",
            backend_choice: GuiGpuBackendChoiceView::from_response(response),
            gpu_status,
            trust_state,
            cpu_confirm_required,
            can_source_exact_probability,
            fallback_reason,
            memory_ticket: GuiGpuMemoryTicketView::from_response(response),
            backpressure: GuiGpuBackpressureView::from_response(response),
        }
    }
}
impl GuiGpuStatusViewModel {
    pub const fn label_i18n_key(&self) -> &'static str {
        self.label_i18n_key
    }
}
impl GuiGpuStatusViewModel {
    pub const fn backend_choice(&self) -> &GuiGpuBackendChoiceView {
        &self.backend_choice
    }
}
impl GuiGpuStatusViewModel {
    pub fn gpu_status(&self) -> &str {
        &self.gpu_status
    }
}
impl GuiGpuStatusViewModel {
    pub const fn trust_state(&self) -> &GuiGpuTrustStateView {
        &self.trust_state
    }
}
impl GuiGpuStatusViewModel {
    pub const fn cpu_confirm_required(&self) -> bool {
        self.cpu_confirm_required
    }
}
impl GuiGpuStatusViewModel {
    pub const fn can_source_exact_probability(&self) -> bool {
        self.can_source_exact_probability
    }
}
impl GuiGpuStatusViewModel {
    pub const fn exact_badge(&self) -> bool {
        self.trust_state.exact_badge()
    }
}
impl GuiGpuStatusViewModel {
    pub const fn fallback_reason(&self) -> &GuiGpuFallbackReasonView {
        &self.fallback_reason
    }
}
impl GuiGpuStatusViewModel {
    pub const fn memory_ticket(&self) -> &GuiGpuMemoryTicketView {
        &self.memory_ticket
    }
}
impl GuiGpuStatusViewModel {
    pub const fn backpressure(&self) -> &GuiGpuBackpressureView {
        &self.backpressure
    }
}
