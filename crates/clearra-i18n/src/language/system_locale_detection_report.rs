#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SystemLocaleSource {
    UserSelected,
    StoredPreference,
    Environment,
    WindowsUserDefaultLocale,
    PosixLocale,
    #[default]
    Unavailable,
}

impl SystemLocaleSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UserSelected => "user-selected",
            Self::StoredPreference => "stored-preference",
            Self::Environment => "environment",
            Self::WindowsUserDefaultLocale => "windows-user-default-locale",
            Self::PosixLocale => "posix-locale",
            Self::Unavailable => "unavailable",
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SystemLocaleDetectionReport {
    locale: Option<String>,
    source: SystemLocaleSource,
}

impl SystemLocaleDetectionReport {
    pub fn new(locale: Option<impl Into<String>>, source: SystemLocaleSource) -> Self {
        Self {
            locale: locale.map(Into::into),
            source,
        }
    }
}
impl SystemLocaleDetectionReport {
    pub fn unavailable() -> Self {
        Self::new(None::<String>, SystemLocaleSource::Unavailable)
    }
}
impl SystemLocaleDetectionReport {
    pub fn environment(locale: impl Into<String>) -> Self {
        Self::new(Some(locale), SystemLocaleSource::Environment)
    }
}
impl SystemLocaleDetectionReport {
    pub fn locale(&self) -> Option<&str> {
        self.locale.as_deref()
    }
}
impl SystemLocaleDetectionReport {
    pub fn source(&self) -> SystemLocaleSource {
        self.source
    }
}
