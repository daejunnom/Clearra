use super::{
    LanguageId, LanguagePreference, LanguageResolutionReport, SystemLocaleDetector,
    SystemLocaleSource,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LanguageResolver;

impl LanguageResolver {
    pub fn resolve(preference: &LanguagePreference) -> LanguageId {
        preference.resolve()
    }
}
impl LanguageResolver {
    pub fn resolve_from_selected(selected: Option<LanguageId>) -> LanguageId {
        Self::resolve_report_from_selected(selected).language()
    }
}
impl LanguageResolver {
    pub fn resolve_report_from_selected(selected: Option<LanguageId>) -> LanguageResolutionReport {
        if let Some(language) = selected.filter(|language| language.is_released()) {
            return LanguageResolutionReport::new(
                language,
                SystemLocaleSource::UserSelected,
                None::<String>,
            );
        }

        let locale_report = SystemLocaleDetector::detect_report();
        let preference = LanguagePreference::new(None, locale_report.locale());

        LanguageResolutionReport::new(
            preference.resolve(),
            locale_report.source(),
            locale_report.locale().map(ToOwned::to_owned),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn planned_language_cannot_bypass_release_status_through_explicit_resolution() {
        let report = LanguageResolver::resolve_report_from_selected(Some(LanguageId::Ja));

        assert_ne!(report.language(), LanguageId::Ja);
        assert_ne!(report.source(), SystemLocaleSource::UserSelected);
    }
}
