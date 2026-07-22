#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PostGpuTrustState {
    TrustedDeterministic,
    TrustedCpuSampleConfirmed,
    Unavailable,
    RejectedMismatch,
}

impl PostGpuTrustState {
    pub const fn can_claim_exact(self) -> bool {
        matches!(
            self,
            Self::TrustedDeterministic | Self::TrustedCpuSampleConfirmed
        )
    }
}
