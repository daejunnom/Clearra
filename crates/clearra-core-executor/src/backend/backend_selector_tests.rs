use clearra_pc_graph::request::{
    GpuDeviceSelection, PcCountPolicy, PcExecutionPolicy, RequestedSearchBackend,
};

use super::*;

struct FixedGpuCapabilityProvider(GpuSearchCapability);

impl SearchBackendCapabilityProvider for FixedGpuCapabilityProvider {
    fn gpu_capability(
        &self,
        _device: GpuDeviceSelection,
    ) -> Result<GpuSearchCapability, CapabilityQueryError> {
        Ok(self.0)
    }

    fn prepared_gpu_capability(
        &self,
        _device: GpuDeviceSelection,
    ) -> Result<GpuSearchCapability, CapabilityQueryError> {
        Ok(self.0)
    }
}

mod case_auto_selects_geometry_exact_cover_without_fallback {
    use super::*;

    #[test]
    fn auto_selects_geometry_exact_cover_without_fallback() {
        let provider = FixedGpuCapabilityProvider(GpuSearchCapability::unavailable(
            GpuUnavailableReason::DeviceNotFound,
        ));
        let selection = PcBackendSelector::select_with_test_dependencies(
            &PcExecutionPolicy::mvp_default(),
            PcBackendSelectionContext::default(),
            &provider,
            PcBackendAvailability {
                cpu_parallel_geometry_exact_cover_feature_enabled: false,
            },
        )
        .expect("selection");

        assert_eq!(selection.requested_backend(), RequestedSearchBackend::Auto);
        assert_eq!(
            selection.selected_backend(),
            SelectedSearchBackend::CpuGeometryExactCover
        );
        assert_eq!(
            selection.selected_model(),
            SearchTraversalModel::BitsetAlgorithmX
        );
        assert_eq!(selection.compute_device(), ComputeDeviceKind::Cpu);
        assert_eq!(
            selection.selection_reason(),
            SearchBackendSelectionReason::AutoCpuGeometryExactCoverBaseline
        );
        assert_eq!(
            selection.result_model(),
            SearchResultModel::GeometryCandidateSet
        );
        assert_eq!(
            selection.solution_trace_mode(),
            BackendSolutionTraceMode::None
        );
        assert!(!selection.state_count_available());
        assert!(!selection.multiplicity_count_available());
        assert!(!selection.backend_fallback_used());
    }
}

mod case_auto_parallel_backend_preserves_trace_enumeration {
    use super::*;

    #[test]
    fn auto_parallel_backend_preserves_trace_enumeration() {
        let policy = PcExecutionPolicy::mvp_default().with_workers(4);
        let provider = FixedGpuCapabilityProvider(GpuSearchCapability::unavailable(
            GpuUnavailableReason::DeviceNotFound,
        ));
        let selection = PcBackendSelector::select_with_test_dependencies(
            &policy,
            PcBackendSelectionContext::opening(true, 10),
            &provider,
            PcBackendAvailability {
                cpu_parallel_geometry_exact_cover_feature_enabled: true,
            },
        )
        .expect("selection");

        assert_eq!(
            selection.selected_backend(),
            SelectedSearchBackend::CpuParallelGeometryExactCover
        );
        assert_eq!(
            selection.selection_reason(),
            SearchBackendSelectionReason::AutoCpuParallelExact
        );
    }
}

mod case_auto_keeps_cpu_geometry_exact_cover_for_small_scenarios_with_gpu_available {
    use super::*;

    #[test]
    fn auto_keeps_cpu_geometry_exact_cover_for_small_scenarios_with_gpu_available() {
        let provider = FixedGpuCapabilityProvider(GpuSearchCapability::available(7));
        let selection = PcBackendSelector::select_with_test_dependencies(
            &PcExecutionPolicy::mvp_default(),
            PcBackendSelectionContext::scenario(PcCountPolicy::CountAll, 6),
            &provider,
            PcBackendAvailability {
                cpu_parallel_geometry_exact_cover_feature_enabled: false,
            },
        )
        .expect("selection");

        assert_eq!(
            selection.selected_backend(),
            SelectedSearchBackend::CpuGeometryExactCover
        );
        assert_eq!(
            selection.selection_reason(),
            SearchBackendSelectionReason::AutoSmallScenarioCpuGeometryExactCover
        );
        assert!(selection.gpu_device().is_none());
    }
}

mod case_auto_uses_parallel_exact_backend_for_large_search {
    use super::*;

    #[test]
    fn auto_uses_parallel_exact_backend_for_large_search() {
        let policy = PcExecutionPolicy::mvp_default().with_workers(4);
        let provider = FixedGpuCapabilityProvider(GpuSearchCapability::unavailable(
            GpuUnavailableReason::DeviceNotFound,
        ));
        let selection = PcBackendSelector::select_with_test_dependencies(
            &policy,
            PcBackendSelectionContext::scenario(PcCountPolicy::CountAll, 15),
            &provider,
            PcBackendAvailability {
                cpu_parallel_geometry_exact_cover_feature_enabled: true,
            },
        )
        .expect("selection");

        assert_eq!(
            selection.selected_backend(),
            SelectedSearchBackend::CpuParallelGeometryExactCover
        );
        assert_eq!(
            selection.selection_reason(),
            SearchBackendSelectionReason::AutoCpuParallelExact
        );
    }
}

mod case_auto_uses_parallel_exact_backend_for_materialized_multiset_family {
    use super::*;

    #[test]
    fn auto_uses_parallel_exact_backend_for_materialized_multiset_family() {
        let policy = PcExecutionPolicy::mvp_default().with_workers(4);
        let provider = FixedGpuCapabilityProvider(GpuSearchCapability::available(7));
        let selection = PcBackendSelector::select_with_test_dependencies(
            &policy,
            PcBackendSelectionContext::scenario(PcCountPolicy::CountAll, 4)
                .with_multiset_group_count(50),
            &provider,
            PcBackendAvailability {
                cpu_parallel_geometry_exact_cover_feature_enabled: true,
            },
        )
        .expect("selection");

        assert_eq!(
            selection.selected_backend(),
            SelectedSearchBackend::CpuParallelGeometryExactCover
        );
        assert_eq!(
            selection.selection_reason(),
            SearchBackendSelectionReason::AutoCpuParallelExact
        );
        assert!(selection.gpu_device().is_none());
    }
}

mod case_auto_requires_benchmark_qualified_gpu {
    use super::*;

    #[test]
    fn auto_requires_benchmark_qualified_gpu() {
        let provider = FixedGpuCapabilityProvider(GpuSearchCapability::available_explicit_only(7));
        let selection = PcBackendSelector::select_with_test_dependencies(
            &PcExecutionPolicy::mvp_default(),
            PcBackendSelectionContext::scenario(PcCountPolicy::CountAll, 10),
            &provider,
            PcBackendAvailability {
                cpu_parallel_geometry_exact_cover_feature_enabled: true,
            },
        )
        .expect("selection");

        assert_eq!(
            selection.selected_backend(),
            SelectedSearchBackend::CpuParallelGeometryExactCover
        );
        assert_eq!(
            selection.selection_reason(),
            SearchBackendSelectionReason::AutoCpuParallelExact
        );
    }
}

mod case_cpu_parallel_geometry_exact_cover_contract_uses_partitioned_algorithm_x {
    use super::*;

    #[test]
    fn cpu_parallel_geometry_exact_cover_contract_uses_partitioned_algorithm_x() {
        assert_eq!(
            SelectedSearchBackend::CpuParallelGeometryExactCover.result_model(),
            SearchResultModel::GeometryCandidateSet
        );
        assert_eq!(
            SelectedSearchBackend::CpuParallelGeometryExactCover.solution_trace_mode(),
            BackendSolutionTraceMode::None
        );
        assert_eq!(
            SelectedSearchBackend::CpuParallelGeometryExactCover.traversal_model(),
            SearchTraversalModel::ImmutableGeometryGraphBuildabilityTasks
        );
        assert_eq!(
            SelectedSearchBackend::CpuParallelGeometryExactCover.compute_device(),
            ComputeDeviceKind::Cpu
        );
        assert!(!SelectedSearchBackend::CpuParallelGeometryExactCover.state_count_available());
        assert!(
            !SelectedSearchBackend::CpuParallelGeometryExactCover.multiplicity_count_available()
        );
    }
}

mod case_explicit_cpu_uses_parallel_geometry_exact_cover_for_large_search {
    use super::*;

    #[test]
    fn explicit_cpu_uses_parallel_geometry_exact_cover_for_large_search() {
        let policy = PcExecutionPolicy::mvp_default()
            .with_backend(RequestedSearchBackend::Cpu)
            .with_workers(8);
        let selection = PcBackendSelector::select_with_context_and_availability(
            &policy,
            PcBackendSelectionContext::scenario(PcCountPolicy::CountAll, 15),
            PcBackendAvailability {
                cpu_parallel_geometry_exact_cover_feature_enabled: true,
            },
        )
        .expect("selection");

        assert_eq!(selection.requested_backend(), RequestedSearchBackend::Cpu);
        assert_eq!(
            selection.selected_backend(),
            SelectedSearchBackend::CpuParallelGeometryExactCover
        );
        assert_eq!(
            selection.selected_model(),
            SearchTraversalModel::ImmutableGeometryGraphBuildabilityTasks
        );
        assert_eq!(selection.compute_device(), ComputeDeviceKind::Cpu);
        assert_eq!(selection.workers_requested(), Some(8));
        assert_eq!(
            selection.workers_used(),
            crate::execution_worker_limit::clamp_requested_workers(8, false)
        );
        assert_eq!(
            selection.selection_reason(),
            SearchBackendSelectionReason::UserRequested
        );
    }
}

mod case_explicit_cpu_threads_override_small_search_parallel_heuristic {
    use super::*;

    #[test]
    fn explicit_cpu_threads_override_small_search_parallel_heuristic() {
        let policy = PcExecutionPolicy::mvp_default()
            .with_backend(RequestedSearchBackend::Cpu)
            .with_workers(4);
        let provider = FixedGpuCapabilityProvider(GpuSearchCapability::available(0));
        let selection = PcBackendSelector::select_with_test_dependencies(
            &policy,
            PcBackendSelectionContext::scenario(PcCountPolicy::CountAll, 4),
            &provider,
            PcBackendAvailability {
                cpu_parallel_geometry_exact_cover_feature_enabled: true,
            },
        )
        .expect("selection");

        assert_eq!(
            selection.selected_backend(),
            SelectedSearchBackend::CpuParallelGeometryExactCover
        );
        assert_eq!(selection.workers_requested(), Some(4));
        assert_eq!(selection.compute_device(), ComputeDeviceKind::Cpu);
        assert!(selection.gpu_device().is_none());
    }
}

mod case_gpu_device_not_found_falls_back_to_cpu_with_reason {
    use super::*;

    #[test]
    fn gpu_device_not_found_falls_back_to_cpu_with_reason() {
        let policy = PcExecutionPolicy::mvp_default()
            .with_backend(RequestedSearchBackend::Gpu)
            .with_allow_backend_fallback(true);
        let provider = FixedGpuCapabilityProvider(GpuSearchCapability::unavailable(
            GpuUnavailableReason::DeviceNotFound,
        ));
        let selection = PcBackendSelector::select_with_context_and_provider(
            &policy,
            PcBackendSelectionContext::default(),
            &provider,
        )
        .expect("selection");

        assert_eq!(selection.requested_backend(), RequestedSearchBackend::Gpu);
        assert_eq!(
            selection.selected_backend(),
            SelectedSearchBackend::CpuGeometryExactCover
        );
        assert_eq!(
            selection.selected_model(),
            SearchTraversalModel::BitsetAlgorithmX
        );
        assert_eq!(selection.compute_device(), ComputeDeviceKind::Cpu);
        assert_eq!(
            selection.fallback_reason(),
            Some(SearchBackendFallbackReason::GpuDeviceNotFound)
        );
        assert_eq!(
            selection.gpu_unavailable_reason(),
            Some(GpuUnavailableReason::DeviceNotFound)
        );
        assert_eq!(
            selection.selection_reason(),
            SearchBackendSelectionReason::ExplicitFallbackToCpuGeometryExactCover
        );
    }
}

mod case_gpu_device_not_found_uses_parallel_cpu_for_large_multiset_family {
    use super::*;

    #[test]
    fn gpu_device_not_found_uses_parallel_cpu_for_large_multiset_family() {
        let policy = PcExecutionPolicy::mvp_default()
            .with_backend(RequestedSearchBackend::Gpu)
            .with_workers(4)
            .with_allow_backend_fallback(true);
        let provider = FixedGpuCapabilityProvider(GpuSearchCapability::unavailable(
            GpuUnavailableReason::DeviceNotFound,
        ));
        let selection = PcBackendSelector::select_with_test_dependencies(
            &policy,
            PcBackendSelectionContext::scenario(PcCountPolicy::CountAll, 4)
                .with_multiset_group_count(50),
            &provider,
            PcBackendAvailability {
                cpu_parallel_geometry_exact_cover_feature_enabled: true,
            },
        )
        .expect("selection");

        assert_eq!(
            selection.selected_backend(),
            SelectedSearchBackend::CpuParallelGeometryExactCover
        );
        assert_eq!(
            selection.selection_reason(),
            SearchBackendSelectionReason::ExplicitFallbackToCpuParallelExact
        );
        assert_eq!(
            selection.fallback_reason(),
            Some(SearchBackendFallbackReason::GpuDeviceNotFound)
        );
    }
}

mod case_gpu_available_selects_gpu {
    use super::*;

    #[test]
    fn gpu_available_selects_gpu() {
        let policy = PcExecutionPolicy::mvp_default()
            .with_backend(RequestedSearchBackend::Gpu)
            .with_allow_backend_fallback(true);
        let provider = FixedGpuCapabilityProvider(GpuSearchCapability::available(0));
        let selection = PcBackendSelector::select_with_context_and_provider(
            &policy,
            PcBackendSelectionContext::default(),
            &provider,
        )
        .expect("selection");

        assert_eq!(selection.requested_backend(), RequestedSearchBackend::Gpu);
        assert_eq!(selection.selected_backend(), SelectedSearchBackend::Gpu);
        assert_eq!(selection.compute_device(), ComputeDeviceKind::Gpu);
        assert!(selection.gpu_available());
        assert_eq!(selection.fallback_reason(), None);
        assert_eq!(
            selection.selection_reason(),
            SearchBackendSelectionReason::UserRequested
        );

        let hybrid_policy = PcExecutionPolicy::mvp_default()
            .with_backend(RequestedSearchBackend::Hybrid)
            .with_allow_backend_fallback(false);
        let hybrid_selection = PcBackendSelector::select_with_context_and_provider(
            &hybrid_policy,
            PcBackendSelectionContext::default(),
            &provider,
        )
        .expect("prepared GPU selection");
        assert_eq!(
            hybrid_selection.selected_backend(),
            SelectedSearchBackend::Gpu
        );
        assert_eq!(
            hybrid_selection.selection_reason(),
            SearchBackendSelectionReason::HybridGpuReady
        );
    }
}

mod case_gpu_kernel_unavailable_falls_back_to_cpu_with_reason {
    use super::*;

    #[test]
    fn gpu_kernel_unavailable_falls_back_to_cpu_with_reason() {
        let policy = PcExecutionPolicy::mvp_default()
            .with_backend(RequestedSearchBackend::Gpu)
            .with_allow_backend_fallback(true);
        let provider = FixedGpuCapabilityProvider(GpuSearchCapability::unavailable(
            GpuUnavailableReason::KernelUnavailable,
        ));
        let selection = PcBackendSelector::select_with_context_and_provider(
            &policy,
            PcBackendSelectionContext::default(),
            &provider,
        )
        .expect("selection");

        assert_eq!(selection.requested_backend(), RequestedSearchBackend::Gpu);
        assert_eq!(
            selection.selected_backend(),
            SelectedSearchBackend::CpuGeometryExactCover
        );
        assert_eq!(
            selection.fallback_reason(),
            Some(SearchBackendFallbackReason::GpuKernelUnavailable)
        );
        assert_eq!(
            selection.selection_reason(),
            SearchBackendSelectionReason::ExplicitFallbackToCpuGeometryExactCover
        );
        assert_eq!(
            selection.gpu_unavailable_reason(),
            Some(GpuUnavailableReason::KernelUnavailable)
        );
        assert!(selection.gpu_failure().is_some());
    }
}

mod case_no_backend_fallback_returns_error {
    use super::*;

    #[test]
    fn no_backend_fallback_returns_error() {
        let policy = PcExecutionPolicy::mvp_default()
            .with_backend(RequestedSearchBackend::Gpu)
            .with_allow_backend_fallback(false);
        let provider = FixedGpuCapabilityProvider(GpuSearchCapability::unavailable(
            GpuUnavailableReason::KernelUnavailable,
        ));
        let selection = PcBackendSelector::select_with_context_and_provider(
            &policy,
            PcBackendSelectionContext::default(),
            &provider,
        );

        assert_eq!(
            selection,
            Err(BackendSelectionError::GpuUnavailable(
                GpuUnavailableReason::KernelUnavailable
            ))
        );
    }
}

mod case_user_facing_hybrid_uses_cpu_when_gpu_is_not_ready {
    use super::*;

    #[test]
    fn user_facing_hybrid_uses_cpu_when_gpu_is_not_ready() {
        let policy = PcExecutionPolicy::mvp_default()
            .with_backend(RequestedSearchBackend::Hybrid)
            .with_allow_backend_fallback(false);
        let provider = FixedGpuCapabilityProvider(GpuSearchCapability::unavailable(
            GpuUnavailableReason::KernelUnavailable,
        ));
        let selection = PcBackendSelector::select_with_context_and_provider(
            &policy,
            PcBackendSelectionContext::default(),
            &provider,
        )
        .expect("hybrid CPU selection");

        assert_eq!(
            selection.selected_backend(),
            SelectedSearchBackend::CpuGeometryExactCover
        );
        assert_eq!(
            selection.selection_reason(),
            SearchBackendSelectionReason::HybridGpuNotReadyCpu
        );
        assert_eq!(selection.fallback_reason(), None);
    }
}

mod case_no_backend_fallback_errors_for_unsupported_backend {
    use super::*;

    #[test]
    fn no_backend_fallback_errors_for_unsupported_backend() {
        let policy = PcExecutionPolicy::mvp_default()
            .with_backend(RequestedSearchBackend::Gpu)
            .with_allow_backend_fallback(false);

        let provider = FixedGpuCapabilityProvider(GpuSearchCapability::unavailable(
            GpuUnavailableReason::KernelUnavailable,
        ));
        assert_eq!(
            PcBackendSelector::select_with_context_and_provider(
                &policy,
                PcBackendSelectionContext::default(),
                &provider
            ),
            Err(BackendSelectionError::GpuUnavailable(
                GpuUnavailableReason::KernelUnavailable
            ))
        );
    }
}
