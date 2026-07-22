use crate::{catalog::TranslationKey, language::LanguageId, TranslationCatalog};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LocalizedText {
    key: TranslationKey,
    language: LanguageId,
    text: String,
}

impl LocalizedText {
    pub fn resolve(
        key: TranslationKey,
        fallback_en: &'static str,
        catalog: TranslationCatalog,
    ) -> Self {
        Self {
            key: key.clone(),
            language: catalog.language(),
            text: catalog.get_or_fallback(&key, fallback_en).to_owned(),
        }
    }
}
impl LocalizedText {
    pub fn key(&self) -> &TranslationKey {
        &self.key
    }
}
impl LocalizedText {
    pub fn language(&self) -> LanguageId {
        self.language
    }
}
impl LocalizedText {
    pub fn text(&self) -> &str {
        &self.text
    }
}
