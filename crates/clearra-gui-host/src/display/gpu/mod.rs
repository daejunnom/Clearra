mod gpu_backend_choice_view;
mod gpu_backpressure_view;
mod gpu_fallback_reason_view;
mod gpu_memory_ticket_view;
mod gpu_status_view_model;
mod gpu_trust_state_view;

pub use gpu_backend_choice_view::GuiGpuBackendChoiceView;
pub use gpu_backpressure_view::GuiGpuBackpressureView;
pub use gpu_fallback_reason_view::GuiGpuFallbackReasonView;
pub use gpu_memory_ticket_view::GuiGpuMemoryTicketView;
pub use gpu_status_view_model::GuiGpuStatusViewModel;
pub use gpu_trust_state_view::GuiGpuTrustStateView;

pub(crate) fn visible_reason(value: Option<String>) -> Option<String> {
    let value = value?;
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("none")
        || trimmed.eq_ignore_ascii_case("null")
        || trimmed.eq_ignore_ascii_case("not_available")
        || trimmed.eq_ignore_ascii_case("not-available")
    {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

pub(crate) fn bool_value(value: Option<String>) -> Option<bool> {
    value.and_then(|value| value.parse().ok())
}

pub(crate) fn u32_value(value: Option<String>) -> u32 {
    value.and_then(|value| value.parse().ok()).unwrap_or(0)
}
