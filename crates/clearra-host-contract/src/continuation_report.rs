#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ContinuationReport {
    available: bool,
    token: Option<String>,
}

impl ContinuationReport {
    pub fn new(available: bool, token: Option<impl Into<String>>) -> Self {
        Self {
            available,
            token: token.map(Into::into),
        }
    }
}
impl ContinuationReport {
    pub const fn available(&self) -> bool {
        self.available
    }
}
impl ContinuationReport {
    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
    }
}
