#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LocalePolicy {
    language: Option<String>,
}

impl LocalePolicy {
    pub fn new(language: Option<impl Into<String>>) -> Self {
        Self {
            language: language.map(Into::into),
        }
    }
}
impl LocalePolicy {
    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    /// Returns heap bytes retained by the optional language string using its
    /// actual allocation capacity. The inline option and policy are excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        Some(
            self.language
                .as_ref()
                .map_or(0, |language| language.capacity() as u128),
        )
    }
}

impl Default for LocalePolicy {
    fn default() -> Self {
        Self::new(None::<String>)
    }
}

#[cfg(test)]
mod retained_capacity_tests {
    use super::LocalePolicy;

    #[test]
    fn retained_capacity_counts_optional_language_string_slack() {
        let mut language = String::with_capacity(79);
        language.push_str("ko-KR");
        let expected = language.capacity() as u128;
        let policy = LocalePolicy::new(Some(language));

        assert_eq!(policy.checked_retained_capacity_bytes(), Some(expected));
        assert_eq!(
            LocalePolicy::new(None::<String>).checked_retained_capacity_bytes(),
            Some(0)
        );
    }
}
