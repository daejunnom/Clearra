use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LanguageId {
    #[default]
    En,
    Ko,
}

impl LanguageId {
    pub const ALL: [Self; 2] = [Self::En, Self::Ko];
}
impl LanguageId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Ko => "ko",
        }
    }
}
impl LanguageId {
    pub fn native_label(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::Ko => "한국어",
        }
    }
}
impl LanguageId {
    pub fn english_label(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::Ko => "Korean",
        }
    }
}
impl LanguageId {
    pub fn parse(value: &str) -> Option<Self> {
        match normalize_language(value).as_str() {
            "en" | "en-us" | "en-gb" => Some(Self::En),
            "ko" | "ko-kr" => Some(Self::Ko),
            _ => None,
        }
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
        assert_eq!(LanguageId::parse("jp"), None);
    }
}
