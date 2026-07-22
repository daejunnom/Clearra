#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostGpuCapabilityState {
    Connected,
    Unavailable,
    RejectedMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PostGpuCapability {
    state: PostGpuCapabilityState,
    exact_supported: bool,
    unavailable_reason: Option<String>,
}

impl PostGpuCapability {
    pub(crate) fn connected_exact() -> Self {
        Self {
            state: PostGpuCapabilityState::Connected,
            exact_supported: true,
            unavailable_reason: None,
        }
    }

    pub(crate) fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            state: PostGpuCapabilityState::Unavailable,
            exact_supported: false,
            unavailable_reason: Some(reason.into()),
        }
    }

    pub(crate) fn rejected_mismatch() -> Self {
        Self {
            state: PostGpuCapabilityState::RejectedMismatch,
            exact_supported: false,
            unavailable_reason: None,
        }
    }

    pub fn state(&self) -> PostGpuCapabilityState {
        self.state
    }

    pub fn runtime_connected(&self) -> bool {
        self.state == PostGpuCapabilityState::Connected
    }

    pub fn exact_supported(&self) -> bool {
        self.exact_supported
    }

    pub fn unavailable_reason(&self) -> Option<&str> {
        self.unavailable_reason.as_deref()
    }
}
