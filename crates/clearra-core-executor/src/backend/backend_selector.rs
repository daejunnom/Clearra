use clearra_pc_graph::request::{BackendFallbackPolicy, PcExecutionPolicy, RequestedSearchBackend};

use super::{
    backend_types::{
        BackendSolutionTraceMode, ComputeDeviceKind, GpuDeviceSummary, GpuUnavailableReason,
        SearchBackendFallbackReason, SearchBackendSelectionReason, SearchResultModel,
        SearchTraversalModel, SelectedSearchBackend,
    },
    CapabilityQueryError, CpuParallelGeometryExactCoverBackend, GpuExecutionFailure,
    GpuExecutionFailureResolution, GpuExecutionFailureStage, GpuSearchCapability,
    NativeSearchBackendCapabilityProvider, SearchBackendCapabilityProvider,
};

mod availability {
    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) struct PcBackendAvailability {
        pub(super) cpu_parallel_geometry_exact_cover_feature_enabled: bool,
    }

    impl PcBackendAvailability {
        pub(super) fn runtime() -> Self {
            Self {
                cpu_parallel_geometry_exact_cover_feature_enabled:
                    CpuParallelGeometryExactCoverBackend::capability().is_supported(),
            }
        }
    }
}
mod backend_choice {
    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) struct BackendChoice {
        pub(super) selected_backend: SelectedSearchBackend,
        pub(super) selection_reason: SearchBackendSelectionReason,
    }
}
mod choice_policy {
    use super::{backend_choice::BackendChoice, request_mapping::selected_backend_from_request, *};

    pub(super) fn select_backend_choice(
        policy: &PcExecutionPolicy,
        context: PcBackendSelectionContext,
        fallback_reason: Option<SearchBackendFallbackReason>,
        gpu_available: bool,
        cpu_parallel_available: bool,
    ) -> BackendChoice {
        if !matches!(policy.requested_backend(), RequestedSearchBackend::Auto) {
            let hybrid_requested =
                matches!(policy.requested_backend(), RequestedSearchBackend::Hybrid);
            let hybrid_cpu_selection = hybrid_requested && !gpu_available;
            let fallback_to_parallel_cpu = fallback_reason.is_some()
                && cpu_parallel_available
                && policy.workers() > 1
                && (context.parallel_cpu_worthwhile() || policy.workers_requested().is_some());
            let hybrid_parallel_cpu = hybrid_cpu_selection
                && cpu_parallel_available
                && policy.workers() > 1
                && (context.parallel_cpu_worthwhile() || policy.workers_requested().is_some());
            let selected_backend = if hybrid_parallel_cpu {
                SelectedSearchBackend::CpuParallelGeometryExactCover
            } else if hybrid_cpu_selection {
                SelectedSearchBackend::CpuGeometryExactCover
            } else if fallback_to_parallel_cpu {
                SelectedSearchBackend::CpuParallelGeometryExactCover
            } else if fallback_reason.is_some() {
                SelectedSearchBackend::CpuGeometryExactCover
            } else if matches!(policy.requested_backend(), RequestedSearchBackend::Cpu)
                && cpu_parallel_available
                && policy.workers() > 1
                && (context.parallel_cpu_worthwhile() || policy.workers_requested().is_some())
            {
                SelectedSearchBackend::CpuParallelGeometryExactCover
            } else if matches!(
                policy.requested_backend(),
                RequestedSearchBackend::Gpu | RequestedSearchBackend::Hybrid
            ) && gpu_available
            {
                SelectedSearchBackend::Gpu
            } else {
                selected_backend_from_request(policy.requested_backend())
                    .unwrap_or(SelectedSearchBackend::None)
            };
            return BackendChoice {
                selected_backend,
                selection_reason: if hybrid_requested && gpu_available {
                    SearchBackendSelectionReason::HybridGpuReady
                } else if hybrid_cpu_selection {
                    SearchBackendSelectionReason::HybridGpuNotReadyCpu
                } else if fallback_to_parallel_cpu {
                    SearchBackendSelectionReason::ExplicitFallbackToCpuParallelExact
                } else if fallback_reason.is_some() {
                    SearchBackendSelectionReason::ExplicitFallbackToCpuGeometryExactCover
                } else {
                    SearchBackendSelectionReason::UserRequested
                },
            };
        }

        if context.small_scenario() {
            if cpu_parallel_available && policy.workers() > 1 && context.parallel_cpu_worthwhile() {
                return BackendChoice {
                    selected_backend: SelectedSearchBackend::CpuParallelGeometryExactCover,
                    selection_reason: SearchBackendSelectionReason::AutoCpuParallelExact,
                };
            }
            return BackendChoice {
                selected_backend: SelectedSearchBackend::CpuGeometryExactCover,
                selection_reason:
                    SearchBackendSelectionReason::AutoSmallScenarioCpuGeometryExactCover,
            };
        }
        if gpu_available {
            return BackendChoice {
                selected_backend: SelectedSearchBackend::Gpu,
                selection_reason: SearchBackendSelectionReason::AutoGpuExactConnected,
            };
        }
        if cpu_parallel_available && policy.workers() > 1 && context.parallel_cpu_worthwhile() {
            return BackendChoice {
                selected_backend: SelectedSearchBackend::CpuParallelGeometryExactCover,
                selection_reason: SearchBackendSelectionReason::AutoCpuParallelExact,
            };
        }
        BackendChoice {
            selected_backend: SelectedSearchBackend::CpuGeometryExactCover,
            selection_reason: SearchBackendSelectionReason::AutoCpuGeometryExactCoverBaseline,
        }
    }
}
mod context {
    use clearra_pc_graph::request::PcCountPolicy;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum PcBackendSelectionContext {
        Opening {
            trace_enumeration_requested: bool,
            piece_window: usize,
            multiset_group_count: usize,
        },
        Scenario {
            count_policy: PcCountPolicy,
            piece_window: usize,
            multiset_group_count: usize,
        },
        Unknown,
    }

    impl PcBackendSelectionContext {
        pub fn opening(trace_enumeration_requested: bool, piece_window: usize) -> Self {
            Self::Opening {
                trace_enumeration_requested,
                piece_window,
                multiset_group_count: 1,
            }
        }
    }
    impl PcBackendSelectionContext {
        pub fn scenario(count_policy: PcCountPolicy, piece_window: usize) -> Self {
            Self::Scenario {
                count_policy,
                piece_window,
                multiset_group_count: 1,
            }
        }
    }
    impl PcBackendSelectionContext {
        pub fn with_multiset_group_count(self, multiset_group_count: usize) -> Self {
            let multiset_group_count = multiset_group_count.max(1);
            match self {
                Self::Opening {
                    trace_enumeration_requested,
                    piece_window,
                    ..
                } => Self::Opening {
                    trace_enumeration_requested,
                    piece_window,
                    multiset_group_count,
                },
                Self::Scenario {
                    count_policy,
                    piece_window,
                    ..
                } => Self::Scenario {
                    count_policy,
                    piece_window,
                    multiset_group_count,
                },
                Self::Unknown => Self::Unknown,
            }
        }

        pub(super) fn small_scenario(self) -> bool {
            matches!(
                self,
                Self::Opening { piece_window, .. } | Self::Scenario { piece_window, .. }
                    if piece_window <= 6
            )
        }

        pub(super) fn large_search(self) -> bool {
            matches!(
                self,
                Self::Opening { piece_window, .. } | Self::Scenario { piece_window, .. }
                    if piece_window > 6
            )
        }

        pub(super) fn parallel_cpu_worthwhile(self) -> bool {
            const PARALLEL_MULTISET_GROUP_THRESHOLD: usize = 8;
            self.large_search()
                || matches!(
                    self,
                    Self::Opening {
                        multiset_group_count,
                        ..
                    } | Self::Scenario {
                        multiset_group_count,
                        ..
                    } if multiset_group_count >= PARALLEL_MULTISET_GROUP_THRESHOLD
                )
        }
    }
    impl Default for PcBackendSelectionContext {
        fn default() -> Self {
            Self::Unknown
        }
    }
}
mod error {
    use super::{CapabilityQueryError, GpuUnavailableReason};

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum BackendSelectionError {
        BackendUnsupported,
        GpuUnavailable(GpuUnavailableReason),
        CapabilityQueryFailed(CapabilityQueryError),
    }

    impl BackendSelectionError {
        pub const fn reason(self) -> &'static str {
            match self {
                Self::BackendUnsupported => "backend_unsupported",
                Self::GpuUnavailable(reason) => reason.as_str(),
                Self::CapabilityQueryFailed(CapabilityQueryError::BindingUnavailable) => {
                    "gpu_binding_unavailable"
                }
                Self::CapabilityQueryFailed(_) => "gpu_capability_query_failed",
            }
        }
    }
}
mod gpu_capability {
    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) struct GpuCapabilityEvaluation {
        pub(super) queried: bool,
        pub(super) available: bool,
        pub(super) auto_selectable: bool,
        pub(super) unavailable_reason: Option<GpuUnavailableReason>,
        pub(super) query_error: Option<CapabilityQueryError>,
    }

    impl GpuCapabilityEvaluation {
        pub(super) const fn not_queried() -> Self {
            Self {
                queried: false,
                available: false,
                auto_selectable: false,
                unavailable_reason: None,
                query_error: None,
            }
        }
    }

    pub(super) fn evaluate_gpu_capability(
        policy: &PcExecutionPolicy,
        provider: &impl SearchBackendCapabilityProvider,
    ) -> GpuCapabilityEvaluation {
        if !matches!(
            policy.requested_backend(),
            RequestedSearchBackend::Auto
                | RequestedSearchBackend::Gpu
                | RequestedSearchBackend::Hybrid
        ) {
            return GpuCapabilityEvaluation::not_queried();
        }

        let capability_span = crate::performance::SearchStageSpan::begin(
            crate::performance::ExecutorSearchStage::PackingGpuCapabilityQuery,
        );
        let capability = if matches!(policy.requested_backend(), RequestedSearchBackend::Hybrid) {
            provider.prepared_gpu_capability(policy.gpu_device().clone())
        } else {
            provider.gpu_capability(policy.gpu_device().clone())
        };
        capability_span.finish(1);
        match capability {
            Ok(capability @ GpuSearchCapability::Available { .. }) => GpuCapabilityEvaluation {
                queried: true,
                available: true,
                auto_selectable: capability.is_auto_selectable(),
                unavailable_reason: None,
                query_error: None,
            },
            Ok(GpuSearchCapability::Unavailable(reason)) => GpuCapabilityEvaluation {
                queried: true,
                available: false,
                auto_selectable: false,
                unavailable_reason: Some(reason),
                query_error: None,
            },
            Err(error) => GpuCapabilityEvaluation {
                queried: true,
                available: false,
                auto_selectable: false,
                unavailable_reason: Some(unavailable_reason_from_query_error(error)),
                query_error: Some(error),
            },
        }
    }

    fn unavailable_reason_from_query_error(error: CapabilityQueryError) -> GpuUnavailableReason {
        match error {
            CapabilityQueryError::BindingUnavailable => GpuUnavailableReason::BindingUnavailable,
            CapabilityQueryError::InvalidArgument
            | CapabilityQueryError::AbiMismatch { .. }
            | CapabilityQueryError::InvalidNativeStatus(_)
            | CapabilityQueryError::InvalidNativeCapability => {
                GpuUnavailableReason::CapabilityQueryFailed
            }
        }
    }

    pub(super) fn fallback_reason_from_gpu_unavailable(
        reason: GpuUnavailableReason,
    ) -> SearchBackendFallbackReason {
        match reason {
            GpuUnavailableReason::FeatureDisabled => {
                SearchBackendFallbackReason::GpuFeatureDisabled
            }
            GpuUnavailableReason::BindingUnavailable => {
                SearchBackendFallbackReason::GpuBindingUnavailable
            }
            GpuUnavailableReason::DeviceNotFound => SearchBackendFallbackReason::GpuDeviceNotFound,
            GpuUnavailableReason::KernelUnavailable => {
                SearchBackendFallbackReason::GpuKernelUnavailable
            }
            GpuUnavailableReason::BackendNotConnected => {
                SearchBackendFallbackReason::GpuBackendNotConnected
            }
            GpuUnavailableReason::ExactSearchUnsupported => {
                SearchBackendFallbackReason::GpuExactSearchUnsupported
            }
            GpuUnavailableReason::CapabilityQueryFailed => {
                SearchBackendFallbackReason::GpuCapabilityQueryFailed
            }
        }
    }
}
mod report {
    use super::*;

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct PcBackendSelection {
        pub(super) requested_backend: RequestedSearchBackend,
        pub(super) selected_backend: SelectedSearchBackend,
        pub(super) selected_model: SearchTraversalModel,
        pub(super) compute_device: ComputeDeviceKind,
        pub(super) selection_reason: SearchBackendSelectionReason,
        pub(super) fallback_reason: Option<SearchBackendFallbackReason>,
        pub(super) workers_requested: Option<usize>,
        pub(super) workers_used: usize,
        pub(super) deterministic_order: bool,
        pub(super) gpu_device: Option<GpuDeviceSummary>,
        pub(super) gpu_available: bool,
        pub(super) gpu_unavailable_reason: Option<GpuUnavailableReason>,
        pub(super) gpu_failure: Option<GpuExecutionFailureResolution>,
        pub(super) backend_fallback: BackendFallbackPolicy,
        pub(super) max_frontier_states: usize,
        pub(super) max_candidates: usize,
        pub(super) max_memory_mib: Option<u64>,
    }

    impl PcBackendSelection {
        pub fn requested_backend(&self) -> RequestedSearchBackend {
            self.requested_backend
        }

        pub fn selected_backend(&self) -> SelectedSearchBackend {
            self.selected_backend
        }

        pub fn selected_model(&self) -> SearchTraversalModel {
            self.selected_model
        }

        pub fn compute_device(&self) -> ComputeDeviceKind {
            self.compute_device
        }

        pub fn selection_reason(&self) -> SearchBackendSelectionReason {
            self.selection_reason
        }

        pub fn fallback_reason(&self) -> Option<SearchBackendFallbackReason> {
            self.fallback_reason
        }

        pub fn backend_fallback(&self) -> BackendFallbackPolicy {
            self.backend_fallback
        }

        pub fn backend_fallback_used(&self) -> bool {
            self.fallback_reason.is_some()
        }

        pub fn workers_requested(&self) -> Option<usize> {
            self.workers_requested
        }

        pub fn workers_used(&self) -> usize {
            self.workers_used
        }

        pub fn deterministic_order(&self) -> bool {
            self.deterministic_order
        }

        pub fn gpu_device(&self) -> Option<&GpuDeviceSummary> {
            self.gpu_device.as_ref()
        }

        pub fn gpu_available(&self) -> bool {
            self.gpu_available
        }

        pub fn gpu_unavailable_reason(&self) -> Option<GpuUnavailableReason> {
            self.gpu_unavailable_reason
        }

        pub fn gpu_failure(&self) -> Option<GpuExecutionFailureResolution> {
            self.gpu_failure
        }

        pub fn max_frontier_states(&self) -> usize {
            self.max_frontier_states
        }

        pub fn max_candidates(&self) -> usize {
            self.max_candidates
        }

        pub fn max_memory_mib(&self) -> Option<u64> {
            self.max_memory_mib
        }

        pub fn result_model(&self) -> SearchResultModel {
            self.selected_backend.result_model()
        }

        pub fn solution_trace_mode(&self) -> BackendSolutionTraceMode {
            self.selected_backend.solution_trace_mode()
        }

        pub fn state_count_available(&self) -> bool {
            self.selected_backend.state_count_available()
        }

        pub fn multiplicity_count_available(&self) -> bool {
            self.selected_backend.multiplicity_count_available()
        }
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct SearchBackendReport {
        selection: PcBackendSelection,
        actual_backend: SelectedSearchBackend,
        fallback_reason: Option<SearchBackendFallbackReason>,
        gpu_failure: Option<GpuExecutionFailureResolution>,
        workers_used: usize,
    }

    impl SearchBackendReport {
        pub(crate) fn from_execution(
            mut selection: PcBackendSelection,
            actual_backend: SelectedSearchBackend,
            execution_fallback_reason: Option<SearchBackendFallbackReason>,
            execution_gpu_failure: Option<GpuExecutionFailureResolution>,
            execution_gpu_device: Option<GpuDeviceSummary>,
            execution_workers_used: usize,
        ) -> Self {
            let workers_used = if actual_backend == SelectedSearchBackend::None {
                0
            } else {
                execution_workers_used.max(1)
            };
            let fallback_reason = execution_fallback_reason.or(selection.fallback_reason);
            let gpu_failure = execution_gpu_failure.or(selection.gpu_failure);
            if execution_gpu_device.is_some() {
                selection.gpu_device = execution_gpu_device;
            }
            Self {
                selection,
                actual_backend,
                fallback_reason,
                gpu_failure,
                workers_used,
            }
        }

        pub fn requested_backend(&self) -> RequestedSearchBackend {
            self.selection.requested_backend
        }

        pub fn selected_backend(&self) -> SelectedSearchBackend {
            self.actual_backend
        }

        pub fn selected_model(&self) -> SearchTraversalModel {
            self.actual_backend.traversal_model()
        }

        pub fn compute_device(&self) -> ComputeDeviceKind {
            self.actual_backend.compute_device()
        }

        pub fn selection_reason(&self) -> SearchBackendSelectionReason {
            if self.actual_backend != self.selection.selected_backend
                && self.fallback_reason.is_some()
            {
                SearchBackendSelectionReason::ExplicitFallbackToCpuGeometryExactCover
            } else {
                self.selection.selection_reason
            }
        }

        pub fn fallback_reason(&self) -> Option<SearchBackendFallbackReason> {
            self.fallback_reason
        }

        pub fn backend_fallback(&self) -> BackendFallbackPolicy {
            self.selection.backend_fallback
        }

        pub fn backend_fallback_used(&self) -> bool {
            self.fallback_reason.is_some()
        }

        pub fn workers_requested(&self) -> Option<usize> {
            self.selection.workers_requested
        }

        pub fn workers_used(&self) -> usize {
            self.workers_used
        }

        pub fn deterministic_order(&self) -> bool {
            self.selection.deterministic_order
        }

        pub fn gpu_device(&self) -> Option<&GpuDeviceSummary> {
            self.selection.gpu_device.as_ref()
        }

        pub fn gpu_available(&self) -> bool {
            self.selection.gpu_available
        }

        pub fn gpu_unavailable_reason(&self) -> Option<GpuUnavailableReason> {
            self.selection.gpu_unavailable_reason
        }

        pub fn gpu_failure(&self) -> Option<GpuExecutionFailureResolution> {
            self.gpu_failure
        }

        pub fn max_frontier_states(&self) -> usize {
            self.selection.max_frontier_states
        }

        pub fn max_candidates(&self) -> usize {
            self.selection.max_candidates
        }

        pub fn max_memory_mib(&self) -> Option<u64> {
            self.selection.max_memory_mib
        }

        pub fn result_model(&self) -> SearchResultModel {
            self.actual_backend.result_model()
        }

        pub fn solution_trace_mode(&self) -> BackendSolutionTraceMode {
            self.actual_backend.solution_trace_mode()
        }

        pub fn state_count_available(&self) -> bool {
            self.actual_backend.state_count_available()
        }

        pub fn multiplicity_count_available(&self) -> bool {
            self.actual_backend.multiplicity_count_available()
        }
    }
}
mod request_mapping {
    use super::*;

    pub(super) fn selected_backend_from_request(
        requested: RequestedSearchBackend,
    ) -> Option<SelectedSearchBackend> {
        match requested {
            RequestedSearchBackend::Auto | RequestedSearchBackend::Cpu => {
                Some(SelectedSearchBackend::CpuGeometryExactCover)
            }
            RequestedSearchBackend::Gpu => Some(SelectedSearchBackend::Gpu),
            RequestedSearchBackend::Hybrid => Some(SelectedSearchBackend::Gpu),
        }
    }
}
mod selector {
    use super::{
        availability::PcBackendAvailability,
        choice_policy::select_backend_choice,
        gpu_capability::{
            evaluate_gpu_capability, fallback_reason_from_gpu_unavailable, GpuCapabilityEvaluation,
        },
        unsupported_reason::unsupported_reason,
        worker_allocation::workers_used_for_backend,
        *,
    };

    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct PcBackendSelector;

    impl PcBackendSelector {
        pub fn select_with_context(
            policy: &PcExecutionPolicy,
            context: PcBackendSelectionContext,
        ) -> Result<PcBackendSelection, BackendSelectionError> {
            Self::select_with_context_and_provider_and_availability(
                policy,
                context,
                &NativeSearchBackendCapabilityProvider,
                PcBackendAvailability::runtime(),
            )
        }
    }
    impl PcBackendSelector {
        pub fn select_with_context_and_provider(
            policy: &PcExecutionPolicy,
            context: PcBackendSelectionContext,
            provider: &impl SearchBackendCapabilityProvider,
        ) -> Result<PcBackendSelection, BackendSelectionError> {
            Self::select_with_context_and_provider_and_availability(
                policy,
                context,
                provider,
                PcBackendAvailability::runtime(),
            )
        }
    }
    impl PcBackendSelector {
        #[cfg(test)]
        pub(super) fn select_with_context_and_availability(
            policy: &PcExecutionPolicy,
            context: PcBackendSelectionContext,
            availability: PcBackendAvailability,
        ) -> Result<PcBackendSelection, BackendSelectionError> {
            Self::select_with_context_and_provider_and_availability(
                policy,
                context,
                &NativeSearchBackendCapabilityProvider,
                availability,
            )
        }

        #[cfg(test)]
        pub(super) fn select_with_test_dependencies(
            policy: &PcExecutionPolicy,
            context: PcBackendSelectionContext,
            provider: &impl SearchBackendCapabilityProvider,
            availability: PcBackendAvailability,
        ) -> Result<PcBackendSelection, BackendSelectionError> {
            Self::select_with_context_and_provider_and_availability(
                policy,
                context,
                provider,
                availability,
            )
        }
    }
    impl PcBackendSelector {
        fn select_with_context_and_provider_and_availability(
            policy: &PcExecutionPolicy,
            context: PcBackendSelectionContext,
            provider: &impl SearchBackendCapabilityProvider,
            availability: PcBackendAvailability,
        ) -> Result<PcBackendSelection, BackendSelectionError> {
            let gpu = if matches!(policy.requested_backend(), RequestedSearchBackend::Auto)
                && context.small_scenario()
            {
                GpuCapabilityEvaluation::not_queried()
            } else {
                evaluate_gpu_capability(policy, provider)
            };
            let mut reason = unsupported_reason(policy, availability);
            if reason.is_none()
                && matches!(policy.requested_backend(), RequestedSearchBackend::Gpu)
                && !gpu.available
            {
                reason = Some(fallback_reason_from_gpu_unavailable(
                    gpu.unavailable_reason
                        .unwrap_or(GpuUnavailableReason::CapabilityQueryFailed),
                ));
            }
            if reason.is_some()
                && !policy.allow_backend_fallback()
                && !matches!(policy.requested_backend(), RequestedSearchBackend::Hybrid)
            {
                if matches!(
                    policy.requested_backend(),
                    RequestedSearchBackend::Gpu | RequestedSearchBackend::Hybrid
                ) {
                    if let Some(error) = gpu.query_error {
                        return Err(BackendSelectionError::CapabilityQueryFailed(error));
                    }
                    return Err(BackendSelectionError::GpuUnavailable(
                        gpu.unavailable_reason
                            .unwrap_or(GpuUnavailableReason::CapabilityQueryFailed),
                    ));
                }
                return Err(BackendSelectionError::BackendUnsupported);
            }
            let choice = select_backend_choice(
                policy,
                context,
                reason,
                gpu.available
                    && (!matches!(policy.requested_backend(), RequestedSearchBackend::Auto)
                        || gpu.auto_selectable),
                availability.cpu_parallel_geometry_exact_cover_feature_enabled,
            );
            let gpu_failure = if matches!(policy.requested_backend(), RequestedSearchBackend::Gpu)
                && !gpu.available
            {
                reason.map(|reason| {
                    GpuExecutionFailure::unavailable(
                        GpuExecutionFailureStage::CapabilityQuery,
                        reason,
                    )
                    .resolve(policy.backend_fallback())
                })
            } else {
                None
            };
            Ok(PcBackendSelection {
                requested_backend: policy.requested_backend(),
                selected_backend: choice.selected_backend,
                selected_model: choice.selected_backend.traversal_model(),
                compute_device: choice.selected_backend.compute_device(),
                selection_reason: choice.selection_reason,
                fallback_reason: reason,
                workers_requested: policy.workers_requested(),
                workers_used: workers_used_for_backend(choice.selected_backend, policy),
                deterministic_order: policy.deterministic(),
                gpu_device: gpu
                    .queried
                    .then(|| GpuDeviceSummary::from_selection(policy.gpu_device())),
                gpu_available: gpu.available,
                gpu_unavailable_reason: gpu.unavailable_reason,
                gpu_failure,
                backend_fallback: policy.backend_fallback(),
                max_frontier_states: policy.max_frontier_states(),
                max_candidates: policy.max_candidates(),
                max_memory_mib: policy.max_memory_mib(),
            })
        }
    }
}
mod unsupported_reason {
    use super::{availability::PcBackendAvailability, *};

    pub(super) fn unsupported_reason(
        policy: &PcExecutionPolicy,
        availability: PcBackendAvailability,
    ) -> Option<SearchBackendFallbackReason> {
        let _ = availability;
        match policy.requested_backend() {
            RequestedSearchBackend::Auto
            | RequestedSearchBackend::Cpu
            | RequestedSearchBackend::Gpu
            | RequestedSearchBackend::Hybrid => None,
        }
    }
}
mod worker_allocation {
    use super::*;

    pub(super) fn workers_used_for_backend(
        selected_backend: SelectedSearchBackend,
        policy: &PcExecutionPolicy,
    ) -> usize {
        match selected_backend {
            SelectedSearchBackend::CpuParallelGeometryExactCover
            | SelectedSearchBackend::Gpu
            | SelectedSearchBackend::Hybrid => policy.workers(),
            SelectedSearchBackend::None | SelectedSearchBackend::CpuGeometryExactCover => 1,
        }
    }
}

pub use context::PcBackendSelectionContext;
pub use error::BackendSelectionError;
pub use report::{PcBackendSelection, SearchBackendReport};
pub use selector::PcBackendSelector;

#[cfg(test)]
use availability::PcBackendAvailability;

#[cfg(test)]
#[path = "backend_selector_tests.rs"]
mod tests;
