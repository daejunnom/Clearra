use super::{SystemLocaleDetectionReport, SystemLocaleSource};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SystemLocaleEnvDetector;

impl SystemLocaleEnvDetector {
    pub const ENV_KEYS: [&'static str; 5] =
        ["CLEARRA_LANG", "LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"];
}
impl SystemLocaleEnvDetector {
    pub fn detect() -> Option<String> {
        Self::detect_report().locale().map(ToOwned::to_owned)
    }
}
impl SystemLocaleEnvDetector {
    pub fn detect_report() -> SystemLocaleDetectionReport {
        Self::detect_report_from_pairs(Self::ENV_KEYS.into_iter().filter_map(|name| {
            std::env::var(name)
                .ok()
                .map(|value| (name.to_owned(), value))
        }))
    }
}
impl SystemLocaleEnvDetector {
    pub fn detect_report_from_pairs<I, K, V>(pairs: I) -> SystemLocaleDetectionReport
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let pairs = pairs
            .into_iter()
            .map(|(key, value)| (key.as_ref().to_owned(), value.as_ref().to_owned()))
            .collect::<Vec<_>>();

        for expected_key in Self::ENV_KEYS {
            for (key, value) in &pairs {
                if key == expected_key {
                    let value = value.trim();
                    if !value.is_empty() {
                        return SystemLocaleDetectionReport::new(
                            Some(value.to_owned()),
                            SystemLocaleSource::Environment,
                        );
                    }
                }
            }
        }

        SystemLocaleDetectionReport::unavailable()
    }
}

#[cfg(test)]
#[path = "system_locale_env_detector_tests.rs"]
mod tests;
