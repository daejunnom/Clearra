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
}
