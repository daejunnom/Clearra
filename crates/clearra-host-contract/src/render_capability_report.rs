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

    /// Returns only the heap payload retained by the optional unsupported
    /// reason, measured from its actual allocation capacity.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        Some(
            self.unsupported_reason
                .as_ref()
                .map_or(0, |reason| reason.capacity() as u128),
        )
    }
}

#[cfg(test)]
mod retained_capacity_tests {
    use super::RenderCapabilityReport;

    #[test]
    fn retained_capacity_counts_unsupported_reason() {
        let mut reason = String::with_capacity(128);
        reason.push_str("renderer_not_in_wasm_artifact");
        let expected = reason.capacity() as u128;
        let report = RenderCapabilityReport::new(false, false, false, Some(reason));

        assert_eq!(report.checked_retained_capacity_bytes(), Some(expected));
    }
}
