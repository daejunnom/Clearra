use super::*;

#[test]
fn selected_language_wins_over_system_locale() {
    let preference = LanguagePreference::new(Some(LanguageId::En), Some("ko-KR"));

    assert_eq!(preference.resolve(), LanguageId::En);
}

#[test]
fn korean_system_locale_resolves_to_korean() {
    let preference = LanguagePreference::new(None, Some("ko_KR.UTF-8"));

    assert_eq!(preference.resolve(), LanguageId::Ko);
}

#[test]
fn unknown_or_absent_locale_defaults_to_english() {
    assert_eq!(LanguagePreference::default().resolve(), LanguageId::En);
    assert_eq!(
        LanguagePreference::new(None, Some("fr-FR")).resolve(),
        LanguageId::En
    );
}

#[test]
fn released_japanese_locale_resolves_from_system_and_explicit_preferences() {
    assert_eq!(LanguageId::parse_known("ja_JP.UTF-8"), Some(LanguageId::Ja));
    assert_eq!(LanguageId::parse("ja_JP.UTF-8"), None);
    assert_eq!(
        LanguagePreference::new(None, Some("ja_JP.UTF-8")).resolve(),
        LanguageId::Ja
    );
    assert_eq!(
        LanguagePreference::new(Some(LanguageId::Ja), None::<String>).resolve(),
        LanguageId::Ja
    );
}
