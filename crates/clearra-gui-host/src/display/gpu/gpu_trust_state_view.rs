use clearra_app::AppResponse;

use super::super::{field_value, first_field};
use super::bool_value;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiGpuTrustStateView {
    trust_state: String,
    status_message: &'static str,
    exact_badge: bool,
    can_source_exact_probability: bool,
}

impl GuiGpuTrustStateView {
    pub fn from_response(response: &AppResponse) -> Self {
        let trust_state = first_field(response, &["gpu_trust_state", "gpu_worker_trust_state"])
            .unwrap_or_else(|| "not-used".to_owned());
        let explicit_exact = bool_value(field_value(response, "gpu_can_source_exact_probability"));
        Self::from_trust_state(trust_state, explicit_exact)
    }
}
impl GuiGpuTrustStateView {
    pub fn from_trust_state(trust_state: String, explicit_exact: Option<bool>) -> Self {
        let normalized = trust_state.as_str();
        let (status_message, exact_badge) = match normalized {
            "gpu-computed-unconfirmed" => ("GPU result needs CPU confirmation", false),
            "gpu-computed-cpu-confirmed" => ("CPU-confirmed GPU result", true),
            "deterministic-reference-matched" => ("Reference backend matched CPU", true),
            "fallback" | "fallback-used" => ("GPU fallback used", false),
            "unavailable" => ("GPU unavailable", false),
            "gpu-computed-mismatch" => ("GPU result mismatched CPU confirmation", false),
            _ => ("GPU not used", false),
        };
        let can_source_exact_probability = explicit_exact.unwrap_or(exact_badge) && exact_badge;

        Self {
            trust_state,
            status_message,
            exact_badge,
            can_source_exact_probability,
        }
    }
}
impl GuiGpuTrustStateView {
    pub fn trust_state(&self) -> &str {
        &self.trust_state
    }
}
impl GuiGpuTrustStateView {
    pub const fn status_message(&self) -> &'static str {
        self.status_message
    }
}
impl GuiGpuTrustStateView {
    pub const fn exact_badge(&self) -> bool {
        self.exact_badge
    }
}
impl GuiGpuTrustStateView {
    pub const fn can_source_exact_probability(&self) -> bool {
        self.can_source_exact_probability
    }
}
