use super::{
    SystemLocaleDetectionReport, SystemLocaleEnvDetector, SystemLocalePlatformDetector,
    SystemLocaleSource,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SystemLocaleDetector;

impl SystemLocaleDetector {
    pub fn detect() -> Option<String> {
        Self::detect_report().locale().map(ToOwned::to_owned)
    }
}
impl SystemLocaleDetector {
    pub fn detect_report() -> SystemLocaleDetectionReport {
        let env_report = SystemLocaleEnvDetector::detect_report();
        if env_report.source() != SystemLocaleSource::Unavailable {
            return env_report;
        }

        SystemLocalePlatformDetector::detect_report()
    }
}
