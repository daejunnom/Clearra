#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DomainError {
    message: String,
}

impl DomainError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}
impl DomainError {
    pub fn message(&self) -> &str {
        &self.message
    }
}
