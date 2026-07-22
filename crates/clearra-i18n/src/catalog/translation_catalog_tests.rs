use super::*;

#[test]
fn korean_catalog_falls_back_to_english_label_when_key_is_missing() {
    let catalog = TranslationCatalog::korean();
    let key = TranslationKey::new("ui.unknown");

    assert_eq!(catalog.get_or_fallback(&key, "Fallback"), "Fallback");
}

#[test]
fn built_in_catalogs_share_key_universe() {
    for key in TranslationCatalog::all_keys() {
        assert!(
            english_catalog::get(key).is_some(),
            "missing English key {key}"
        );
        assert!(
            korean_catalog::get(key).is_some(),
            "missing Korean key {key}"
        );
    }
}
