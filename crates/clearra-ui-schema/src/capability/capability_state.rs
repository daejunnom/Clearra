use clearra_validation::{Mvp2CapabilityState, Mvp3CapabilityState};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CapabilityState {
    Unsupported,
    ConnectedApproximate,
    ConnectedExact,
}

impl CapabilityState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unsupported => "Unsupported",
            Self::ConnectedApproximate => "ConnectedApproximate",
            Self::ConnectedExact => "ConnectedExact",
        }
    }
}
impl CapabilityState {
    pub const fn runtime_execution_allowed(self) -> bool {
        matches!(self, Self::ConnectedApproximate | Self::ConnectedExact)
    }
}
impl CapabilityState {
    pub const fn exact_claim_allowed(self) -> bool {
        matches!(self, Self::ConnectedExact)
    }
}
impl CapabilityState {
    pub const fn disabled_reason_required(self) -> bool {
        matches!(self, Self::Unsupported)
    }
}
impl CapabilityState {
    pub const fn from_mvp2_state(state: Mvp2CapabilityState) -> Self {
        match state {
            Mvp2CapabilityState::Unsupported => Self::Unsupported,
            Mvp2CapabilityState::ConnectedApproximate => Self::ConnectedApproximate,
            Mvp2CapabilityState::ConnectedExact => Self::ConnectedExact,
        }
    }
}
impl CapabilityState {
    pub const fn from_mvp3_state(state: Mvp3CapabilityState) -> Self {
        match state {
            Mvp3CapabilityState::Unsupported => Self::Unsupported,
            Mvp3CapabilityState::ConnectedApproximate => Self::ConnectedApproximate,
            Mvp3CapabilityState::ConnectedExact => Self::ConnectedExact,
        }
    }
}

#[cfg(test)]
#[path = "capability_state_tests.rs"]
mod tests;
