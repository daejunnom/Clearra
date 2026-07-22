use super::SystemLocaleDetectionReport;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SystemLocalePlatformDetector;

impl SystemLocalePlatformDetector {
    pub fn detect() -> Option<String> {
        Self::detect_report().locale().map(ToOwned::to_owned)
    }
}
impl SystemLocalePlatformDetector {
    pub fn detect_report() -> SystemLocaleDetectionReport {
        // Rust shell intentionally avoids platform FFI for now. Native GUI
        // shells own OS locale calls.
        SystemLocaleDetectionReport::unavailable()
    }
}
