#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BackendPolicy {
    backend_requested: String,
    allow_backend_fallback: bool,
}

impl BackendPolicy {
    pub fn new(backend_requested: impl Into<String>, allow_backend_fallback: bool) -> Self {
        Self {
            backend_requested: backend_requested.into(),
            allow_backend_fallback,
        }
    }
}
impl BackendPolicy {
    pub fn backend_requested(&self) -> &str {
        &self.backend_requested
    }
}
impl BackendPolicy {
    pub const fn allow_backend_fallback(&self) -> bool {
        self.allow_backend_fallback
    }
}

impl Default for BackendPolicy {
    fn default() -> Self {
        Self::new("auto", true)
    }
}
