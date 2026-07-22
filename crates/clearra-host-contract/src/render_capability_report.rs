#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RenderCapabilityReport {
    png_supported: bool,
    gif_supported: bool,
    render_exact: bool,
    unsupported_reason: Option<String>,
}

impl RenderCapabilityReport {
    pub fn new(
        png_supported: bool,
        gif_supported: bool,
        render_exact: bool,
        unsupported_reason: Option<impl Into<String>>,
    ) -> Self {
        Self {
            png_supported,
            gif_supported,
            render_exact,
            unsupported_reason: unsupported_reason.map(Into::into),
        }
    }

    pub const fn png_supported(&self) -> bool {
        self.png_supported
    }

    pub const fn gif_supported(&self) -> bool {
        self.gif_supported
    }

    pub const fn render_exact(&self) -> bool {
        self.render_exact
    }

    pub fn unsupported_reason(&self) -> Option<&str> {
        self.unsupported_reason.as_deref()
    }
}
