use clearra_app::AppResponse;

use super::super::first_field;
use super::visible_reason;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiGpuFallbackReasonView {
    fallback_reason: String,
    fallback_visible: bool,
    unavailable_reason: String,
    unavailable_visible: bool,
}

impl GuiGpuFallbackReasonView {
    pub fn from_response(response: &AppResponse) -> Self {
        let fallback_reason = visible_reason(first_field(
            response,
            &["gpu_worker_fallback_reason", "backend_fallback_reason"],
        ));
        let unavailable_reason = visible_reason(first_field(
            response,
            &["gpu_worker_unavailable_reason", "gpu_unavailable_reason"],
        ));

        Self {
            fallback_visible: fallback_reason.is_some(),
            fallback_reason: fallback_reason.unwrap_or_else(|| "none".to_owned()),
            unavailable_visible: unavailable_reason.is_some(),
            unavailable_reason: unavailable_reason.unwrap_or_else(|| "none".to_owned()),
        }
    }
}
impl GuiGpuFallbackReasonView {
    pub fn fallback_reason(&self) -> &str {
        &self.fallback_reason
    }
}
impl GuiGpuFallbackReasonView {
    pub const fn fallback_visible(&self) -> bool {
        self.fallback_visible
    }
}
impl GuiGpuFallbackReasonView {
    pub fn unavailable_reason(&self) -> &str {
        &self.unavailable_reason
    }
}
impl GuiGpuFallbackReasonView {
    pub const fn unavailable_visible(&self) -> bool {
        self.unavailable_visible
    }
}
