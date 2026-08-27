use crate::RenderCapabilityReport;

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CapabilityReport {
    app_request_boundary: String,
    executor_boundary: String,
    render_capability: Option<RenderCapabilityReport>,
}

impl CapabilityReport {
    pub fn new(
        app_request_boundary: impl Into<String>,
        executor_boundary: impl Into<String>,
    ) -> Self {
        Self {
            app_request_boundary: app_request_boundary.into(),
            executor_boundary: executor_boundary.into(),
            render_capability: None,
        }
    }

    /// Allocation-free owned-parts seam for a boundary that has already
    /// authorized every retained capability allocation.
    pub fn from_owned_memory_authorized_parts(
        app_request_boundary: String,
        executor_boundary: String,
        render_capability: Option<RenderCapabilityReport>,
    ) -> Self {
        Self {
            app_request_boundary,
            executor_boundary,
            render_capability,
        }
    }
}
impl CapabilityReport {
    pub fn app_request_boundary(&self) -> &str {
        &self.app_request_boundary
    }
}
impl CapabilityReport {
    pub fn executor_boundary(&self) -> &str {
        &self.executor_boundary
    }
}
impl CapabilityReport {
    pub fn with_render_capability(mut self, render_capability: RenderCapabilityReport) -> Self {
        self.render_capability = Some(render_capability);
        self
    }

    pub fn render_capability(&self) -> Option<&RenderCapabilityReport> {
        self.render_capability.as_ref()
    }

    /// Returns only heap payload transitively retained by this capability
    /// report. Boundary and render-reason strings are measured from their
    /// actual allocation capacities; inline owners are excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        let mut bytes = (self.app_request_boundary.capacity() as u128)
            .checked_add(self.executor_boundary.capacity() as u128)?;
        if let Some(render_capability) = &self.render_capability {
            bytes = bytes.checked_add(render_capability.checked_retained_capacity_bytes()?)?;
        }
        Some(bytes)
    }
}

impl Default for CapabilityReport {
    fn default() -> Self {
        Self::new("clearra-app/AppRequest", "validation-before-executor")
    }
}

#[cfg(test)]
mod retained_capacity_tests {
    use super::CapabilityReport;
    use crate::RenderCapabilityReport;

    #[test]
    fn retained_capacity_counts_boundaries_and_nested_render_reason() {
        let mut app_boundary = String::with_capacity(80);
        app_boundary.push_str("clearra-app/AppRequest");
        let app_capacity = app_boundary.capacity() as u128;
        let mut executor_boundary = String::with_capacity(96);
        executor_boundary.push_str("validation-before-executor");
        let executor_capacity = executor_boundary.capacity() as u128;
        let mut reason = String::with_capacity(144);
        reason.push_str("renderer_not_in_wasm_artifact");
        let reason_capacity = reason.capacity() as u128;
        let report = CapabilityReport::new(app_boundary, executor_boundary).with_render_capability(
            RenderCapabilityReport::new(false, false, false, Some(reason)),
        );

        assert_eq!(
            report.checked_retained_capacity_bytes(),
            app_capacity
                .checked_add(executor_capacity)
                .and_then(|bytes| bytes.checked_add(reason_capacity))
        );
    }
}
