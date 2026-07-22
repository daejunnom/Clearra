use super::SearchBackendFallbackReason;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum GpuTrustState {
    #[default]
    NotUsed,
    Unavailable,
    FallbackUsed {
        reason: SearchBackendFallbackReason,
    },
    GpuComputedUnconfirmed,
    GpuComputedCpuConfirmed,
    GpuComputedMismatch,
    DeterministicReferenceMatched,
}

impl GpuTrustState {
    pub const fn can_source_exact_probability(self) -> bool {
        matches!(
            self,
            Self::GpuComputedCpuConfirmed | Self::DeterministicReferenceMatched
        )
    }
}
impl GpuTrustState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotUsed => "not-used",
            Self::Unavailable => "unavailable",
            Self::FallbackUsed { .. } => "fallback-used",
            Self::GpuComputedUnconfirmed => "gpu-computed-unconfirmed",
            Self::GpuComputedCpuConfirmed => "gpu-computed-cpu-confirmed",
            Self::GpuComputedMismatch => "gpu-computed-mismatch",
            Self::DeterministicReferenceMatched => "deterministic-reference-matched",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_trust_state_requires_cpu_confirm_for_exact_probability() {
        assert!(!GpuTrustState::GpuComputedUnconfirmed.can_source_exact_probability());
        assert!(GpuTrustState::GpuComputedCpuConfirmed.can_source_exact_probability());
        assert!(GpuTrustState::DeterministicReferenceMatched.can_source_exact_probability());
        assert!(!GpuTrustState::Unavailable.can_source_exact_probability());
        assert!(!GpuTrustState::GpuComputedMismatch.can_source_exact_probability());
    }
}
