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

    /// Returns only the heap payload retained by the result-kind string,
    /// measured from its actual allocation capacity. The inline `AppResult`
    /// owner is excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        Some(self.kind.capacity() as u128)
    }
}

#[cfg(test)]
mod retained_capacity_tests {
    use super::AppResult;

    #[test]
    fn retained_capacity_counts_result_kind_allocation_capacity() {
        let mut kind = String::with_capacity(96);
        kind.push_str("build-probability");
        let expected = kind.capacity() as u128;
        let result = AppResult::new(kind);

        assert_eq!(result.checked_retained_capacity_bytes(), Some(expected));
    }
}
