use super::*;

#[test]
fn clearra_lang_wins_over_lang_family_environment() {
    let report = SystemLocaleEnvDetector::detect_report_from_pairs([
        ("LANG", "en-US"),
        ("LC_MESSAGES", "fr-FR"),
        ("CLEARRA_LANG", "ko-KR"),
    ]);

    assert_eq!(report.locale(), Some("ko-KR"));
    assert_eq!(report.source(), SystemLocaleSource::Environment);
}

#[test]
fn lang_family_is_used_when_clearra_lang_is_absent() {
    let report = SystemLocaleEnvDetector::detect_report_from_pairs([("LANG", "ko_KR.UTF-8")]);

    assert_eq!(report.locale(), Some("ko_KR.UTF-8"));
}

#[test]
fn empty_environment_values_are_ignored() {
    let report = SystemLocaleEnvDetector::detect_report_from_pairs([
        ("CLEARRA_LANG", " "),
        ("LANG", "en-US"),
    ]);

    assert_eq!(report.locale(), Some("en-US"));
}
