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
}

impl Default for LocalePolicy {
    fn default() -> Self {
        Self::new(None::<String>)
    }
}
