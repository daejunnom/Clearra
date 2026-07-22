use super::{
    two_line_capability::{TwoLineCapability, TwoLineCapabilityInput},
    two_line_fallback_reason::TwoLineFallbackReason,
    two_line_fast_path_availability::TwoLineFastPathAvailability,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TwoLineDispatchDecision {
    UseCoreSearch {
        fallback_reason: Option<TwoLineFallbackReason>,
        capability: TwoLineCapability,
        fast_path: TwoLineFastPathAvailability,
    },
}

impl TwoLineDispatchDecision {
    pub fn from_capability_and_availability(
        capability: TwoLineCapability,
        fast_path: TwoLineFastPathAvailability,
    ) -> Self {
        let fallback_reason = capability
            .fallback_reason()
            .or_else(|| fast_path.fallback_reason());

        Self::UseCoreSearch {
            fallback_reason,
            capability,
            fast_path,
        }
    }
}
impl TwoLineDispatchDecision {
    pub fn reason(self) -> Option<TwoLineFallbackReason> {
        match self {
            Self::UseCoreSearch {
                fallback_reason, ..
            } => fallback_reason,
        }
    }
}
impl TwoLineDispatchDecision {
    pub fn capability(self) -> TwoLineCapability {
        match self {
            Self::UseCoreSearch { capability, .. } => capability,
        }
    }
}
impl TwoLineDispatchDecision {
    pub fn fast_path(self) -> TwoLineFastPathAvailability {
        match self {
            Self::UseCoreSearch { fast_path, .. } => fast_path,
        }
    }
}
impl TwoLineDispatchDecision {
    pub fn fast_path_available(self) -> bool {
        self.fast_path().is_available()
    }
}

pub fn dispatch_two_line(input: TwoLineCapabilityInput) -> TwoLineDispatchDecision {
    dispatch_two_line_with_availability(input, TwoLineFastPathAvailability::current_scope())
}

pub fn dispatch_two_line_with_availability(
    input: TwoLineCapabilityInput,
    fast_path: TwoLineFastPathAvailability,
) -> TwoLineDispatchDecision {
    TwoLineDispatchDecision::from_capability_and_availability(
        TwoLineCapability::evaluate(input),
        fast_path,
    )
}

#[cfg(test)]
#[path = "two_line_dispatch_tests.rs"]
mod tests;
