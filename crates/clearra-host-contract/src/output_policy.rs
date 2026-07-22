#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OutputPolicy {
    format: String,
    include_render_model: bool,
}

impl OutputPolicy {
    pub fn new(format: impl Into<String>, include_render_model: bool) -> Self {
        Self {
            format: format.into(),
            include_render_model,
        }
    }
}
impl OutputPolicy {
    pub fn format(&self) -> &str {
        &self.format
    }
}
impl OutputPolicy {
    pub const fn include_render_model(&self) -> bool {
        self.include_render_model
    }
}

impl Default for OutputPolicy {
    fn default() -> Self {
        Self::new("text", true)
    }
}
