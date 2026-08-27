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

    /// Returns the heap payload retained by the continuation token, measured
    /// from its actual allocation capacity.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        Some(
            self.token
                .as_ref()
                .map_or(0, |token| token.capacity() as u128),
        )
    }
}

#[cfg(test)]
mod retained_capacity_tests {
    use super::ContinuationReport;

    #[test]
    fn retained_capacity_counts_token_capacity() {
        let mut token = String::with_capacity(128);
        token.push_str("next-page");
        let expected = token.capacity() as u128;
        let report = ContinuationReport::new(true, Some(token));

        assert_eq!(report.checked_retained_capacity_bytes(), Some(expected));
    }
}
