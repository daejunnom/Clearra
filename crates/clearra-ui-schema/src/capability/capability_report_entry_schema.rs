use super::CapabilityState;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapabilityReportEntrySchema {
    capability_id: String,
    state: CapabilityState,
    disabled_reason: Option<String>,
}

impl CapabilityReportEntrySchema {
    pub fn new(
        capability_id: impl Into<String>,
        state: CapabilityState,
        disabled_reason: Option<String>,
    ) -> Self {
        Self {
            capability_id: capability_id.into(),
            state,
            disabled_reason,
        }
    }
}
impl CapabilityReportEntrySchema {
    pub fn capability_id(&self) -> &str {
        &self.capability_id
    }
}
impl CapabilityReportEntrySchema {
    pub const fn state(&self) -> CapabilityState {
        self.state
    }
}
impl CapabilityReportEntrySchema {
    pub fn state_label(&self) -> &'static str {
        self.state.as_str()
    }
}
impl CapabilityReportEntrySchema {
    pub fn disabled_reason(&self) -> Option<&str> {
        self.disabled_reason.as_deref()
    }
}
impl CapabilityReportEntrySchema {
    pub const fn runtime_execution_allowed(&self) -> bool {
        self.state.runtime_execution_allowed()
    }
}
impl CapabilityReportEntrySchema {
    pub const fn exact_claim_allowed(&self) -> bool {
        self.state.exact_claim_allowed()
    }
}
impl CapabilityReportEntrySchema {
    pub fn missing_disabled_reason(&self) -> bool {
        self.state.disabled_reason_required() && self.disabled_reason.is_none()
    }
}

#[cfg(test)]
#[path = "capability_report_entry_schema_tests.rs"]
mod tests;
