#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExitCode {
    Success,
    ValidationFailed,
    Unsupported,
    InternalError,
}

impl ExitCode {
    pub const fn code(self) -> i32 {
        match self {
            Self::Success => 0,
            Self::InternalError => 1,
            Self::ValidationFailed => 2,
            Self::Unsupported => 3,
        }
    }
}

impl Default for ExitCode {
    fn default() -> Self {
        Self::Success
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_are_stable_for_cli_contract() {
        assert_eq!(ExitCode::Success.code(), 0);
        assert_eq!(ExitCode::InternalError.code(), 1);
        assert_eq!(ExitCode::ValidationFailed.code(), 2);
        assert_eq!(ExitCode::Unsupported.code(), 3);
    }
}
