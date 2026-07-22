#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvariantError {
    message: String,
}

impl InvariantError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}
impl InvariantError {
    pub fn message(&self) -> &str {
        &self.message
    }
}
