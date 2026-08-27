#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuggestedNextStep {
    text: String,
}

impl SuggestedNextStep {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}
impl SuggestedNextStep {
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns only the heap payload retained by the suggestion text,
    /// measured by `String` allocation capacity. The inline suggestion owner
    /// is excluded.
    pub fn checked_retained_capacity_bytes(&self) -> Option<u128> {
        Some(self.text.capacity() as u128)
    }
}

#[cfg(test)]
mod retained_capacity_tests {
    use super::SuggestedNextStep;

    #[test]
    fn suggestion_retained_capacity_counts_text_allocation_capacity() {
        let mut text = String::with_capacity(144);
        text.push_str("reduce the queue expression");
        let expected = text.capacity() as u128;
        let suggestion = SuggestedNextStep::new(text);

        assert_eq!(suggestion.checked_retained_capacity_bytes(), Some(expected));
    }
}
