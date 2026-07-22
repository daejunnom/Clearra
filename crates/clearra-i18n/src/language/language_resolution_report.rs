use super::{LanguageId, SystemLocaleSource};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LanguageResolutionReport {
    language: LanguageId,
    source: SystemLocaleSource,
    locale: Option<String>,
}

impl LanguageResolutionReport {
    pub fn new(
        language: LanguageId,
        source: SystemLocaleSource,
        locale: Option<impl Into<String>>,
    ) -> Self {
        Self {
            language,
            source,
            locale: locale.map(Into::into),
        }
    }
}
impl LanguageResolutionReport {
    pub fn language(&self) -> LanguageId {
        self.language
    }
}
impl LanguageResolutionReport {
    pub fn source(&self) -> SystemLocaleSource {
        self.source
    }
}
impl LanguageResolutionReport {
    pub fn locale(&self) -> Option<&str> {
        self.locale.as_deref()
    }
}
impl LanguageResolutionReport {
    pub fn language_source(&self) -> &'static str {
        self.source.as_str()
    }
}
