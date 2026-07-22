use std::fmt;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TranslationKey(String);

impl TranslationKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}
impl TranslationKey {
    pub fn ui_backend_label(backend: &str) -> Self {
        Self::new(format!("ui.backend.{backend}.label"))
    }
}
impl TranslationKey {
    pub fn ui_backend_description(backend: &str) -> Self {
        Self::new(format!("ui.backend.{backend}.description"))
    }
}
impl TranslationKey {
    pub fn ui_setup_result(column_id: &str) -> Self {
        Self::new(format!("ui.setup.result.{column_id}"))
    }
}
impl TranslationKey {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&'static str> for TranslationKey {
    fn from(value: &'static str) -> Self {
        Self::new(value)
    }
}

impl fmt::Display for TranslationKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}
