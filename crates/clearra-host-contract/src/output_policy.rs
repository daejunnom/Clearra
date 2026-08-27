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

    /// Returns only the heap payload retained by the output-format string,
    /// measured by `String` allocation capacity. The inline `OutputPolicy` is
    /// excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        Some(self.format.capacity() as u128)
    }
}

impl Default for OutputPolicy {
    fn default() -> Self {
        Self::new("text", true)
    }
}

#[cfg(test)]
mod retained_capacity_tests {
    use super::OutputPolicy;

    #[test]
    fn retained_capacity_counts_format_allocation_capacity() {
        let mut format = String::with_capacity(128);
        format.push_str("json");
        let expected = format.capacity() as u128;
        let policy = OutputPolicy::new(format, false);

        assert_eq!(policy.checked_retained_capacity_bytes(), Some(expected));
    }
}
