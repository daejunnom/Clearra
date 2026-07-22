use clearra_i18n::{LocalizedText, TranslationCatalog, TranslationKey};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalizedLabelSchema {
    key: TranslationKey,
    fallback_en: &'static str,
}

impl LocalizedLabelSchema {
    pub fn new(key: TranslationKey, fallback_en: &'static str) -> Self {
        Self { key, fallback_en }
    }
}
impl LocalizedLabelSchema {
    pub fn key(&self) -> &TranslationKey {
        &self.key
    }
}
impl LocalizedLabelSchema {
    pub fn fallback_en(&self) -> &'static str {
        self.fallback_en
    }
}
impl LocalizedLabelSchema {
    pub fn resolve(&self, catalog: TranslationCatalog) -> LocalizedText {
        LocalizedText::resolve(self.key.clone(), self.fallback_en, catalog)
    }
}

#[cfg(test)]
mod tests {
    use clearra_i18n::{LanguageId, TranslationCatalog};

    use super::*;

    #[test]
    fn localized_label_resolves_through_catalog_with_fallback_label() {
        let label = LocalizedLabelSchema::new(
            TranslationKey::new("ui.setup.result.total_solution_count"),
            "Total solutions",
        );

        let text = label.resolve(TranslationCatalog::new(LanguageId::Ko));

        assert_eq!(label.key().as_str(), "ui.setup.result.total_solution_count");
        assert_eq!(label.fallback_en(), "Total solutions");
        assert_eq!(text.text(), "전체 해법 수");
    }
}
