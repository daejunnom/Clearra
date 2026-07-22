use std::{error::Error, fmt};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcContinuationTokenError {
    message: String,
}

impl PcContinuationTokenError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}
impl PcContinuationTokenError {
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for PcContinuationTokenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for PcContinuationTokenError {}
