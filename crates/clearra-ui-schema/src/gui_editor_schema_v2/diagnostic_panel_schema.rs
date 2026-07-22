#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticPanelSchema {
    fields: Vec<&'static str>,
    unsupported_reason_fields: Vec<&'static str>,
    json_contract_keys_localized: bool,
}

impl DiagnosticPanelSchema {
    pub fn v2() -> Self {
        Self {
            fields: vec![
                "severity",
                "code",
                "message",
                "evidence",
                "suggested_next_step",
                "unsupported_reason",
                "fallback_reason",
                "renderer_capability",
            ],
            unsupported_reason_fields: vec![
                "backend_fallback_reason",
                "gpu_unavailable_reason",
                "hybrid_disabled_reason",
                "unsupported_reason",
                "render_asset_invalid",
            ],
            json_contract_keys_localized: false,
        }
    }
}
impl DiagnosticPanelSchema {
    pub fn fields(&self) -> &[&'static str] {
        &self.fields
    }
}
impl DiagnosticPanelSchema {
    pub fn unsupported_reason_fields(&self) -> &[&'static str] {
        &self.unsupported_reason_fields
    }
}
impl DiagnosticPanelSchema {
    pub const fn json_contract_keys_localized(&self) -> bool {
        self.json_contract_keys_localized
    }
}
impl DiagnosticPanelSchema {
    pub fn exposes_reason_field(&self, field: &str) -> bool {
        self.unsupported_reason_fields
            .iter()
            .any(|known| known == &field)
    }
}
