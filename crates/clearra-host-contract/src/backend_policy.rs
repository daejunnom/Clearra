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

    /// Returns heap bytes retained by the requested-backend string using its
    /// actual allocation capacity. The inline policy owner is excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        Some(self.backend_requested.capacity() as u128)
    }
}

impl Default for BackendPolicy {
    fn default() -> Self {
        Self::new("auto", true)
    }
}

#[cfg(test)]
mod retained_capacity_tests {
    use super::BackendPolicy;

    #[test]
    fn retained_capacity_counts_backend_string_slack() {
        let mut backend = String::with_capacity(73);
        backend.push_str("cpu");
        let expected = backend.capacity() as u128;
        let policy = BackendPolicy::new(backend, false);

        assert_eq!(policy.checked_retained_capacity_bytes(), Some(expected));
    }
}
