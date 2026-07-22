#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackendCapability {
    backend_id: &'static str,
    supported: bool,
    disabled_reason: Option<&'static str>,
}

impl BackendCapability {
    pub const fn supported(backend_id: &'static str) -> Self {
        Self {
            backend_id,
            supported: true,
            disabled_reason: None,
        }
    }
}
impl BackendCapability {
    pub const fn disabled(backend_id: &'static str, reason: &'static str) -> Self {
        Self {
            backend_id,
            supported: false,
            disabled_reason: Some(reason),
        }
    }
}
impl BackendCapability {
    pub fn backend_id(self) -> &'static str {
        self.backend_id
    }
}
impl BackendCapability {
    pub fn is_supported(self) -> bool {
        self.supported
    }
}
impl BackendCapability {
    pub fn disabled_reason(self) -> Option<&'static str> {
        self.disabled_reason
    }
}
