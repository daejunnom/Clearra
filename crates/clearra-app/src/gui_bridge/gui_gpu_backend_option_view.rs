use crate::gui_bridge::GuiBackendCapabilityView;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiGpuBackendOptionView {
    backend_id: &'static str,
    enabled: bool,
    disabled_reason: Option<&'static str>,
    diagnostic_key: Option<&'static str>,
}

impl GuiGpuBackendOptionView {
    pub fn from_backend_capability(capability: &GuiBackendCapabilityView) -> Self {
        Self {
            backend_id: capability.backend_id(),
            enabled: capability.enabled(),
            disabled_reason: capability.disabled_reason_code(),
            diagnostic_key: capability.diagnostic_localization_key(),
        }
    }
}
impl GuiGpuBackendOptionView {
    pub fn backend_options() -> Vec<Self> {
        GuiBackendCapabilityView::backend_options()
            .iter()
            .map(Self::from_backend_capability)
            .collect()
    }
}
impl GuiGpuBackendOptionView {
    pub fn backend_id(&self) -> &'static str {
        self.backend_id
    }
}
impl GuiGpuBackendOptionView {
    pub fn enabled(&self) -> bool {
        self.enabled
    }
}
impl GuiGpuBackendOptionView {
    pub fn disabled_reason(&self) -> Option<&'static str> {
        self.disabled_reason
    }
}
impl GuiGpuBackendOptionView {
    pub fn diagnostic_key(&self) -> Option<&'static str> {
        self.diagnostic_key
    }
}
