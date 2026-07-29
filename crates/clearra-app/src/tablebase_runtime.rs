use clearra_core_executor::{
    install_pc4_compact_tablebase, release_pc4_compact_tablebase, Pc4TablebaseError,
};

#[derive(Debug)]
pub struct AppTablebaseSession {
    installed: bool,
}

impl AppTablebaseSession {
    pub fn install_pc4_compact(artifact: &[u8]) -> Result<Self, AppTablebaseInstallError> {
        install_pc4_compact_tablebase(artifact).map_err(AppTablebaseInstallError::from)?;
        Ok(Self { installed: true })
    }
}

impl Drop for AppTablebaseSession {
    fn drop(&mut self) {
        if self.installed {
            let _ = release_pc4_compact_tablebase();
            self.installed = false;
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppTablebaseInstallError {
    reason: &'static str,
}

impl AppTablebaseInstallError {
    pub const fn reason(&self) -> &'static str {
        self.reason
    }
}

impl From<Pc4TablebaseError> for AppTablebaseInstallError {
    fn from(error: Pc4TablebaseError) -> Self {
        Self {
            reason: error.reason(),
        }
    }
}

impl std::fmt::Display for AppTablebaseInstallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.reason)
    }
}

impl std::error::Error for AppTablebaseInstallError {}

#[cfg(test)]
mod tests {
    use super::AppTablebaseSession;

    #[test]
    fn invalid_tablebase_artifact_is_rejected_at_the_app_boundary() {
        let error = AppTablebaseSession::install_pc4_compact(b"not-a-tablebase")
            .expect_err("invalid artifact must fail closed");
        assert_eq!(error.reason(), "pc4_tablebase_header_invalid");
    }
}
