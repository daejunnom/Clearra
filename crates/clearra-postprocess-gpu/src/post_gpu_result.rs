use crate::{PostBackendRequest, PostGpuCapability, PostGpuTrustState, SearchBackendRequest};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PostGpuResult {
    Connected {
        search_backend_selected: SearchBackendRequest,
        union_words: Vec<u64>,
        capability: PostGpuCapability,
        trust_state: PostGpuTrustState,
        shader_hash: String,
    },
    Unavailable {
        search_backend_selected: SearchBackendRequest,
        capability: PostGpuCapability,
        fallback_used: bool,
        fallback_backend: Option<PostBackendRequest>,
    },
    RejectedMismatch {
        search_backend_selected: SearchBackendRequest,
        capability: PostGpuCapability,
        expected_digest: String,
        actual_digest: String,
    },
}

impl PostGpuResult {
    pub(crate) fn connected(
        search_backend_selected: SearchBackendRequest,
        union_words: Vec<u64>,
        shader_hash: String,
    ) -> Self {
        Self::Connected {
            search_backend_selected,
            union_words,
            capability: PostGpuCapability::connected_exact(),
            trust_state: PostGpuTrustState::TrustedDeterministic,
            shader_hash,
        }
    }

    pub(crate) fn unavailable(
        search_backend_selected: SearchBackendRequest,
        reason: impl Into<String>,
    ) -> Self {
        Self::Unavailable {
            search_backend_selected,
            capability: PostGpuCapability::unavailable(reason),
            fallback_used: false,
            fallback_backend: None,
        }
    }

    pub(crate) fn rejected_mismatch(
        search_backend_selected: SearchBackendRequest,
        expected_digest: impl Into<String>,
        actual_digest: impl Into<String>,
    ) -> Self {
        Self::RejectedMismatch {
            search_backend_selected,
            capability: PostGpuCapability::rejected_mismatch(),
            expected_digest: expected_digest.into(),
            actual_digest: actual_digest.into(),
        }
    }

    pub fn with_cpu_fallback(mut self) -> Self {
        if let Self::Unavailable {
            fallback_used,
            fallback_backend,
            ..
        } = &mut self
        {
            *fallback_used = true;
            *fallback_backend = Some(PostBackendRequest::Cpu);
        }
        self
    }

    pub fn search_backend_selected(&self) -> SearchBackendRequest {
        match self {
            Self::Connected {
                search_backend_selected,
                ..
            }
            | Self::Unavailable {
                search_backend_selected,
                ..
            }
            | Self::RejectedMismatch {
                search_backend_selected,
                ..
            } => *search_backend_selected,
        }
    }

    pub fn post_backend_selected(&self) -> Option<PostBackendRequest> {
        match self {
            Self::Connected { .. } => Some(PostBackendRequest::Gpu),
            Self::Unavailable {
                fallback_backend, ..
            } => *fallback_backend,
            Self::RejectedMismatch { .. } => None,
        }
    }

    pub fn capability(&self) -> &PostGpuCapability {
        match self {
            Self::Connected { capability, .. }
            | Self::Unavailable { capability, .. }
            | Self::RejectedMismatch { capability, .. } => capability,
        }
    }

    pub fn trust_state(&self) -> PostGpuTrustState {
        match self {
            Self::Connected { trust_state, .. } => *trust_state,
            Self::Unavailable { .. } => PostGpuTrustState::Unavailable,
            Self::RejectedMismatch { .. } => PostGpuTrustState::RejectedMismatch,
        }
    }

    pub fn fallback_used(&self) -> bool {
        matches!(
            self,
            Self::Unavailable {
                fallback_used: true,
                ..
            }
        )
    }

    pub fn fallback_reason(&self) -> Option<&str> {
        self.capability().unavailable_reason()
    }

    pub fn union_words(&self) -> Option<&[u64]> {
        match self {
            Self::Connected { union_words, .. } => Some(union_words),
            _ => None,
        }
    }

    pub fn shader_hash(&self) -> Option<&str> {
        match self {
            Self::Connected { shader_hash, .. } => Some(shader_hash),
            _ => None,
        }
    }

    pub fn cpu_confirm_required(&self) -> bool {
        !self.can_claim_exact()
    }

    pub fn can_claim_exact(&self) -> bool {
        matches!(
            self,
            Self::Connected {
                trust_state: PostGpuTrustState::TrustedDeterministic
                    | PostGpuTrustState::TrustedCpuSampleConfirmed,
                capability,
                ..
            } if capability.exact_supported()
        )
    }
}

#[cfg(test)]
#[path = "post_gpu_result_tests.rs"]
mod tests;
