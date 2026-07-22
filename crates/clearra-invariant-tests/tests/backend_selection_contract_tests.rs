use clearra_core_executor::backend::{
    BackendSelectionError, CapabilityQueryError, GpuSearchCapability, GpuUnavailableReason,
    PcBackendSelectionContext, PcBackendSelector, SearchBackendCapabilityProvider,
    SearchBackendFallbackReason, SearchBackendSelectionReason, SelectedSearchBackend,
};
use clearra_pc_graph::request::{
    GpuDeviceSelection, PcCountPolicy, PcExecutionPolicy, RequestedSearchBackend,
};

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
fn gpu_fallback_to_cpu_geometry_exact_cover_requires_backend_fallback_reason() {
    let policy = PcExecutionPolicy::mvp_default()
        .with_backend(RequestedSearchBackend::Gpu)
        .with_allow_backend_fallback(true);

    let selection = PcBackendSelector::select_with_context_and_provider(
        &policy,
        PcBackendSelectionContext::default(),
        &UnavailableGpuProvider,
    )
    .expect("explicit fallback is allowed");

    assert_eq!(selection.requested_backend(), RequestedSearchBackend::Gpu);
    assert_eq!(
        selection.selected_backend(),
        SelectedSearchBackend::CpuGeometryExactCover
    );
    assert!(selection.backend_fallback_used());
    assert_eq!(
        selection.fallback_reason(),
        Some(SearchBackendFallbackReason::GpuKernelUnavailable)
    );
    assert_eq!(
        selection.selection_reason(),
        SearchBackendSelectionReason::ExplicitFallbackToCpuGeometryExactCover
    );
}

#[test]
fn gpu_without_backend_fallback_is_not_a_successful_selection_when_unavailable() {
    let policy = PcExecutionPolicy::mvp_default()
        .with_backend(RequestedSearchBackend::Gpu)
        .with_allow_backend_fallback(false);

    assert_eq!(
        PcBackendSelector::select_with_context_and_provider(
            &policy,
            PcBackendSelectionContext::default(),
            &UnavailableGpuProvider
        ),
        Err(BackendSelectionError::GpuUnavailable(
            GpuUnavailableReason::KernelUnavailable
        ))
    );
}

#[test]
fn auto_cpu_geometry_exact_cover_selection_always_reports_an_auto_selection_reason() {
    let baseline = PcBackendSelector::select_with_context(
        &PcExecutionPolicy::mvp_default(),
        Default::default(),
    )
    .expect("auto selection");
    assert_eq!(
        baseline.selected_backend(),
        SelectedSearchBackend::CpuGeometryExactCover
    );
    assert_eq!(
        baseline.selection_reason(),
        SearchBackendSelectionReason::AutoCpuGeometryExactCoverBaseline
    );

    let small_scenario = PcBackendSelector::select_with_context(
        &PcExecutionPolicy::mvp_default(),
        PcBackendSelectionContext::scenario(PcCountPolicy::CountAll, 6),
    )
    .expect("small scenario auto selection");
    assert_eq!(
        small_scenario.selected_backend(),
        SelectedSearchBackend::CpuGeometryExactCover
    );
    assert_eq!(
        small_scenario.selection_reason(),
        SearchBackendSelectionReason::AutoSmallScenarioCpuGeometryExactCover
    );
}
