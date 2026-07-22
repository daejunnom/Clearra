use crate::gui_bridge::gui_backend_capability_view::GuiBackendCapabilityView;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiDisabledReason {
    backend_id: &'static str,
    disabled_reason_code: &'static str,
    diagnostic_source: &'static str,
}

impl GuiDisabledReason {
    pub const fn new(
        backend_id: &'static str,
        disabled_reason_code: &'static str,
        diagnostic_source: &'static str,
    ) -> Self {
        Self {
            backend_id,
            disabled_reason_code,
            diagnostic_source,
        }
    }
}
impl GuiDisabledReason {
    pub fn backend_reasons() -> Vec<Self> {
        GuiBackendCapabilityView::backend_options()
            .into_iter()
            .filter_map(|option| {
                option
                    .disabled_reason_code()
                    .map(|reason| Self::new(option.backend_id(), reason, "clearra-ui-schema"))
            })
            .collect()
    }
}
impl GuiDisabledReason {
    pub fn backend_id(&self) -> &str {
        self.backend_id
    }
}
impl GuiDisabledReason {
    pub fn disabled_reason_code(&self) -> &str {
        self.disabled_reason_code
    }
}
impl GuiDisabledReason {
    pub fn diagnostic_source(&self) -> &str {
        self.diagnostic_source
    }
}
