use crate::language::LanguageId;

use super::{english_catalog, korean_catalog, TranslationKey};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TranslationCatalog {
    language: LanguageId,
}

impl TranslationCatalog {
    pub fn new(language: LanguageId) -> Self {
        Self { language }
    }
}
impl TranslationCatalog {
    pub fn english() -> Self {
        Self::new(LanguageId::En)
    }
}
impl TranslationCatalog {
    pub fn korean() -> Self {
        Self::new(LanguageId::Ko)
    }
}
impl TranslationCatalog {
    pub fn language(self) -> LanguageId {
        self.language
    }
}
impl TranslationCatalog {
    pub fn get(self, key: &TranslationKey) -> Option<&'static str> {
        match self.language {
            LanguageId::En => english_catalog::get(key.as_str()),
            LanguageId::Ko => korean_catalog::get(key.as_str()),
        }
    }
}
impl TranslationCatalog {
    pub fn get_or_fallback(self, key: &TranslationKey, fallback_en: &'static str) -> &'static str {
        self.get(key)
            .or_else(|| english_catalog::get(key.as_str()))
            .unwrap_or(fallback_en)
    }
}
impl TranslationCatalog {
    pub fn all_keys() -> &'static [&'static str] {
        english_catalog::KEYS
    }
}

#[cfg(test)]
#[path = "translation_catalog_tests.rs"]
mod tests;
