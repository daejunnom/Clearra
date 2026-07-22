use crate::backend::GpuTrustState;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuWorkerExactnessGate {
    PrefilterOnly,
    ExactCandidateSource,
    BackendFallback,
    BackendUnavailable,
    RejectedMismatch,
    Unsupported,
}

impl GpuWorkerExactnessGate {
    pub const fn for_trust_state(trust_state: GpuTrustState) -> Self {
        match trust_state {
            GpuTrustState::GpuComputedUnconfirmed => Self::PrefilterOnly,
            GpuTrustState::GpuComputedCpuConfirmed
            | GpuTrustState::DeterministicReferenceMatched => Self::ExactCandidateSource,
            GpuTrustState::FallbackUsed { .. } => Self::BackendFallback,
            GpuTrustState::Unavailable => Self::BackendUnavailable,
            GpuTrustState::GpuComputedMismatch => Self::RejectedMismatch,
            GpuTrustState::NotUsed => Self::Unsupported,
        }
    }
}
impl GpuWorkerExactnessGate {
    pub const fn can_source_exact_probability(self) -> bool {
        matches!(self, Self::ExactCandidateSource)
    }
}
impl GpuWorkerExactnessGate {
    pub const fn can_accept_build_variant(self) -> bool {
        matches!(self, Self::ExactCandidateSource)
    }
}

#[cfg(test)]
mod compile_time_contracts {
    use crate::backend::{GpuTrustState, SearchBackendFallbackReason};

    use super::GpuWorkerExactnessGate;

    const _: () = {
        assert!(matches!(
            GpuWorkerExactnessGate::for_trust_state(GpuTrustState::GpuComputedUnconfirmed),
            GpuWorkerExactnessGate::PrefilterOnly
        ));
        assert!(
            !GpuWorkerExactnessGate::for_trust_state(GpuTrustState::GpuComputedUnconfirmed)
                .can_source_exact_probability()
        );
        assert!(
            !GpuWorkerExactnessGate::for_trust_state(GpuTrustState::GpuComputedUnconfirmed)
                .can_accept_build_variant()
        );
    };

    const _: () = {
        assert!(matches!(
            GpuWorkerExactnessGate::for_trust_state(GpuTrustState::GpuComputedCpuConfirmed),
            GpuWorkerExactnessGate::ExactCandidateSource
        ));
        assert!(
            GpuWorkerExactnessGate::for_trust_state(GpuTrustState::GpuComputedCpuConfirmed)
                .can_source_exact_probability()
        );
    };

    const _: () = {
        assert!(matches!(
            GpuWorkerExactnessGate::for_trust_state(GpuTrustState::DeterministicReferenceMatched),
            GpuWorkerExactnessGate::ExactCandidateSource
        ));
    };

    const _: () = {
        assert!(matches!(
            GpuWorkerExactnessGate::for_trust_state(GpuTrustState::FallbackUsed {
                reason: SearchBackendFallbackReason::GpuFeatureDisabled
            }),
            GpuWorkerExactnessGate::BackendFallback
        ));
        assert!(matches!(
            GpuWorkerExactnessGate::for_trust_state(GpuTrustState::GpuComputedMismatch),
            GpuWorkerExactnessGate::RejectedMismatch
        ));
    };
}
