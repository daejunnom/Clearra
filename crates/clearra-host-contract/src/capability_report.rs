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
}

impl Default for CapabilityReport {
    fn default() -> Self {
        Self::new("clearra-app/AppRequest", "validation-before-executor")
    }
}
