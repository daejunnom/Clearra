use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LanguageId {
    #[default]
    En,
    Ko,
    Ja,
}

impl LanguageId {
    /// Every locale understood by the i18n layer.
    pub const ALL: [Self; 3] = [Self::En, Self::Ko, Self::Ja];

    /// Locales whose CLI, GUI, and Discord surfaces are all ready for use.
    pub const RELEASED: [Self; 3] = [Self::En, Self::Ko, Self::Ja];
}
impl LanguageId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Ko => "ko",
            Self::Ja => "ja",
        }
    }
}
impl LanguageId {
    pub fn native_label(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::Ko => "한국어",
            Self::Ja => "日本語",
        }
    }
}
impl LanguageId {
    pub fn english_label(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::Ko => "Korean",
            Self::Ja => "Japanese",
        }
    }
}
impl LanguageId {
    /// Parses only locales that are safe to expose in released products.
    pub fn parse(value: &str) -> Option<Self> {
        match normalize_language(value).as_str() {
            "en" | "en-us" | "en-gb" => Some(Self::En),
            "ko" | "ko-kr" => Some(Self::Ko),
            "ja" | "ja-jp" => Some(Self::Ja),
            _ => None,
        }
    }

    /// Parses a known locale prefix for catalog and system-locale tooling.
    pub fn parse_known(value: &str) -> Option<Self> {
        let normalized = normalize_language(value);
        match normalized.split(['-', '.']).next() {
            Some("en") => Some(Self::En),
            Some("ko") => Some(Self::Ko),
            Some("ja") => Some(Self::Ja),
            _ => None,
        }
    }

    pub fn is_released(self) -> bool {
        Self::RELEASED.contains(&self)
    }
}

impl fmt::Display for LanguageId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for LanguageId {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value).ok_or(())
    }
}

pub(crate) fn normalize_language(value: &str) -> String {
    value.trim().replace('_', "-").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_language_aliases() {
        assert_eq!(LanguageId::parse("en"), Some(LanguageId::En));
        assert_eq!(LanguageId::parse("en_US"), Some(LanguageId::En));
        assert_eq!(LanguageId::parse("ko-KR"), Some(LanguageId::Ko));
        assert_eq!(LanguageId::parse("en-CA"), None);
        assert_eq!(LanguageId::parse("ko-JP"), None);
        assert_eq!(LanguageId::parse("ja-JP"), Some(LanguageId::Ja));
        assert_eq!(LanguageId::parse_known("ja_JP"), Some(LanguageId::Ja));
        assert_eq!(LanguageId::parse("jp"), None);
        assert_eq!(LanguageId::parse_known("jp"), None);
        assert_eq!(
            LanguageId::RELEASED,
            [LanguageId::En, LanguageId::Ko, LanguageId::Ja]
        );
    }
}
