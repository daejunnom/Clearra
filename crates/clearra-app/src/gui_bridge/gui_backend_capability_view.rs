#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuiBackendCapabilityView {
    backend_id: &'static str,
    label_key: &'static str,
    description_key: &'static str,
    enabled: bool,
    disabled_reason_code: Option<&'static str>,
    diagnostic_localization_key: Option<&'static str>,
    schema_source: &'static str,
}

impl GuiBackendCapabilityView {
    pub const fn new(
        backend_id: &'static str,
        label_key: &'static str,
        description_key: &'static str,
        enabled: bool,
        disabled_reason_code: Option<&'static str>,
        diagnostic_localization_key: Option<&'static str>,
    ) -> Self {
        Self {
            backend_id,
            label_key,
            description_key,
            enabled,
            disabled_reason_code,
            diagnostic_localization_key,
            schema_source: "clearra-ui-schema/setup_explorer/BackendOptionsSchema",
        }
    }
}
impl GuiBackendCapabilityView {
    pub fn backend_options() -> Vec<Self> {
        vec![
            Self::new(
                "auto",
                "ui.backend.auto.label",
                "ui.backend.auto.description",
                true,
                None,
                None,
            ),
            Self::new(
                "cpu",
                "ui.backend.cpu.label",
                "ui.backend.cpu.description",
                true,
                None,
                None,
            ),
            Self::new(
                "gpu",
                "ui.backend.gpu.label",
                "ui.backend.gpu.description",
                true,
                None,
                None,
            ),
            Self::new(
                "hybrid",
                "ui.backend.hybrid.label",
                "ui.backend.hybrid.description",
                true,
                None,
                None,
            ),
        ]
    }
}
impl GuiBackendCapabilityView {
    pub fn backend_id(&self) -> &'static str {
        self.backend_id
    }
}
impl GuiBackendCapabilityView {
    pub fn label_key(&self) -> &'static str {
        self.label_key
    }
}
impl GuiBackendCapabilityView {
    pub fn description_key(&self) -> &'static str {
        self.description_key
    }
}
impl GuiBackendCapabilityView {
    pub fn enabled(&self) -> bool {
        self.enabled
    }
}
impl GuiBackendCapabilityView {
    pub fn disabled_reason_code(&self) -> Option<&'static str> {
        self.disabled_reason_code
    }
}
impl GuiBackendCapabilityView {
    pub fn diagnostic_localization_key(&self) -> Option<&'static str> {
        self.diagnostic_localization_key
    }
}
impl GuiBackendCapabilityView {
    pub fn schema_source(&self) -> &'static str {
        self.schema_source
    }
}
