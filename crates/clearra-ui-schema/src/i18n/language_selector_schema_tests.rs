use super::*;

#[test]
fn language_selector_defaults_to_english_and_exposes_korean_option() {
    let schema = LanguageSelectorSchema::mvp();

    assert_eq!(schema.default_language(), LanguageId::En);
    assert_eq!(schema.resolved_language(), LanguageId::En);
    assert!(schema
        .options()
        .iter()
        .any(|option| option.id() == LanguageId::Ko && option.native_label() == "한국어"));
}

#[test]
fn explicit_selection_wins_over_detected_locale() {
    let preference = LanguagePreference::new(Some(LanguageId::En), Some("ko-KR"));
    let schema = LanguageSelectorSchema::from_preference(&preference);

    assert_eq!(schema.detected_language(), Some(LanguageId::Ko));
    assert_eq!(schema.selected_language(), Some(LanguageId::En));
    assert_eq!(schema.resolved_language(), LanguageId::En);
}
