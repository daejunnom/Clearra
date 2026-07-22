#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DiagnosticsPolicy {
    include_diagnostics: bool,
    fail_on_warnings: bool,
}

impl DiagnosticsPolicy {
    pub const fn new(include_diagnostics: bool, fail_on_warnings: bool) -> Self {
        Self {
            include_diagnostics,
            fail_on_warnings,
        }
    }
}
impl DiagnosticsPolicy {
    pub const fn include_diagnostics(self) -> bool {
        self.include_diagnostics
    }
}
impl DiagnosticsPolicy {
    pub const fn fail_on_warnings(self) -> bool {
        self.fail_on_warnings
    }
}

impl Default for DiagnosticsPolicy {
    fn default() -> Self {
        Self::new(true, false)
    }
}
