use crate::{language::LanguageId, TranslationCatalog, TranslationKey};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TranslationEntry {
    key: TranslationKey,
    text: String,
}

impl TranslationEntry {
    pub fn key(&self) -> &TranslationKey {
        &self.key
    }
}
impl TranslationEntry {
    pub fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UiTranslationExport {
    language: LanguageId,
    entries: Vec<TranslationEntry>,
}

impl UiTranslationExport {
    pub fn for_language(language: LanguageId) -> Self {
        let catalog = TranslationCatalog::new(language);
        let entries = TranslationCatalog::all_keys()
            .iter()
            .map(|key| {
                let key = TranslationKey::new(*key);
                let text = catalog.get_or_fallback(&key, "").to_owned();
                TranslationEntry { key, text }
            })
            .collect();

        Self { language, entries }
    }
}
impl UiTranslationExport {
    pub fn language(&self) -> LanguageId {
        self.language
    }
}
impl UiTranslationExport {
    pub fn entries(&self) -> &[TranslationEntry] {
        &self.entries
    }
}
impl UiTranslationExport {
    pub fn translate(&self, key: &TranslationKey) -> Option<&str> {
        self.entries
            .iter()
            .find(|entry| entry.key() == key)
            .map(TranslationEntry::text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_korean_ui_catalog_without_changing_contract_keys() {
        let export = UiTranslationExport::for_language(LanguageId::Ko);
        let key = TranslationKey::new("ui.setup.result.total_solution_count");

        assert_eq!(export.language(), LanguageId::Ko);
        assert_eq!(export.translate(&key), Some("전체 해법 수"));
        assert!(export
            .entries()
            .iter()
            .any(|entry| entry.key().as_str() == "ui.backend.auto.label"));
    }
}
