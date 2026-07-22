#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AppResult {
    kind: String,
}

impl AppResult {
    pub fn new(kind: impl Into<String>) -> Self {
        Self { kind: kind.into() }
    }
}
impl AppResult {
    pub fn kind(&self) -> &str {
        &self.kind
    }
}
