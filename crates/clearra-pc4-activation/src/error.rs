use core::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActivationError {
    code: &'static str,
}

impl ActivationError {
    pub(crate) const fn new(code: &'static str) -> Self {
        Self { code }
    }

    pub const fn code(self) -> &'static str {
        self.code
    }
}

impl fmt::Display for ActivationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code)
    }
}

impl std::error::Error for ActivationError {}

pub(crate) type Result<T> = core::result::Result<T, ActivationError>;
