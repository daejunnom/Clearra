use super::{PcBackendSelection, SearchBackendFallbackReason, SearchBackendReport};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BackendFallback {
    used: bool,
    reason: Option<SearchBackendFallbackReason>,
}

impl BackendFallback {
    pub fn new(used: bool, reason: Option<SearchBackendFallbackReason>) -> Self {
        Self { used, reason }
    }
}
impl BackendFallback {
    pub fn from_report(report: &SearchBackendReport) -> Self {
        Self::new(report.backend_fallback_used(), report.fallback_reason())
    }
}
impl BackendFallback {
    pub fn from_selection(selection: &PcBackendSelection) -> Self {
        Self::new(
            selection.backend_fallback_used(),
            selection.fallback_reason(),
        )
    }
}
impl BackendFallback {
    pub fn used(self) -> bool {
        self.used
    }
}
impl BackendFallback {
    pub fn reason(self) -> Option<SearchBackendFallbackReason> {
        self.reason
    }
}
impl BackendFallback {
    pub fn reason_label(self) -> &'static str {
        self.reason
            .map_or("none", SearchBackendFallbackReason::as_str)
    }
}

#[cfg(test)]
mod tests {
    use clearra_pc_graph::request::{
        GpuDeviceSelection, PcExecutionPolicy, RequestedSearchBackend,
    };

    use crate::backend::{
        CapabilityQueryError, GpuSearchCapability, GpuUnavailableReason, PcBackendSelectionContext,
        PcBackendSelector, SearchBackendCapabilityProvider,
    };

    use super::*;

    struct UnavailableGpuProvider;

    impl SearchBackendCapabilityProvider for UnavailableGpuProvider {
        fn gpu_capability(
            &self,
            _device: GpuDeviceSelection,
        ) -> Result<GpuSearchCapability, CapabilityQueryError> {
            Ok(GpuSearchCapability::unavailable(
                GpuUnavailableReason::KernelUnavailable,
            ))
        }

        fn prepared_gpu_capability(
            &self,
            _device: GpuDeviceSelection,
        ) -> Result<GpuSearchCapability, CapabilityQueryError> {
            Ok(GpuSearchCapability::unavailable(
                GpuUnavailableReason::KernelUnavailable,
            ))
        }
    }

    #[test]
    fn backend_fallback_reports_explicit_reason_when_selector_falls_back() {
        let policy = PcExecutionPolicy::mvp_default()
            .with_requested_backend(RequestedSearchBackend::Gpu)
            .with_allow_backend_fallback(true);
        let selection = PcBackendSelector::select_with_context_and_provider(
            &policy,
            PcBackendSelectionContext::scenario(
                clearra_pc_graph::request::PcCountPolicy::CountAll,
                12,
            ),
            &UnavailableGpuProvider,
        )
        .expect("fallback allowed");

        let fallback = BackendFallback::from_selection(&selection);

        assert!(fallback.used());
        assert_eq!(fallback.reason_label(), "gpu_kernel_unavailable");
    }
}
