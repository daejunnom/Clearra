#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationEvidence {
    key: String,
    value: String,
}

impl ValidationEvidence {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}
impl ValidationEvidence {
    pub fn key(&self) -> &str {
        &self.key
    }
}
impl ValidationEvidence {
    pub fn value(&self) -> &str {
        &self.value
    }
}
