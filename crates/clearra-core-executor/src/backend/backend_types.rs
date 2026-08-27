use clearra_pc_graph::request::GpuDeviceSelection;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SelectedSearchBackend {
    None,
    CpuGeometryExactCover,
    CpuParallelGeometryExactCover,
    Gpu,
    Hybrid,
}

impl SelectedSearchBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::CpuGeometryExactCover => "cpu-geometry-exact-cover",
            Self::CpuParallelGeometryExactCover => "cpu-parallel-geometry-exact-cover",
            Self::Gpu => "gpu",
            Self::Hybrid => "hybrid",
        }
    }
}
impl SelectedSearchBackend {
    pub fn result_model(self) -> SearchResultModel {
        match self {
            Self::None => SearchResultModel::None,
            Self::CpuGeometryExactCover
            | Self::CpuParallelGeometryExactCover
            | Self::Gpu
            | Self::Hybrid => SearchResultModel::GeometryCandidateSet,
        }
    }
}
impl SelectedSearchBackend {
    pub fn solution_trace_mode(self) -> BackendSolutionTraceMode {
        BackendSolutionTraceMode::None
    }
}
impl SelectedSearchBackend {
    pub fn state_count_available(self) -> bool {
        false
    }
}
impl SelectedSearchBackend {
    pub fn multiplicity_count_available(self) -> bool {
        false
    }
}
impl SelectedSearchBackend {
    pub(super) fn traversal_model(self) -> SearchTraversalModel {
        match self {
            Self::None => SearchTraversalModel::None,
            Self::CpuGeometryExactCover => SearchTraversalModel::BitsetAlgorithmX,
            Self::CpuParallelGeometryExactCover => {
                SearchTraversalModel::ImmutableGeometryGraphBuildabilityTasks
            }
            Self::Gpu | Self::Hybrid => SearchTraversalModel::GpuGeometryExactCoverFrontier,
        }
    }
}
impl SelectedSearchBackend {
    pub(super) fn compute_device(self) -> ComputeDeviceKind {
        match self {
            Self::None => ComputeDeviceKind::None,
            Self::CpuGeometryExactCover | Self::CpuParallelGeometryExactCover => {
                ComputeDeviceKind::Cpu
            }
            Self::Gpu => ComputeDeviceKind::Gpu,
            Self::Hybrid => ComputeDeviceKind::Hybrid,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchResultModel {
    None,
    GeometryCandidateSet,
}

impl SearchResultModel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::GeometryCandidateSet => "geometry-candidate-set",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchBackendSelectionReason {
    AutoCpuGeometryExactCoverBaseline,
    AutoCpuParallelExact,
    AutoGpuExactConnected,
    AutoSmallScenarioCpuGeometryExactCover,
    UserRequested,
    HybridGpuReady,
    HybridGpuNotReadyCpu,
    ExplicitFallbackToCpuGeometryExactCover,
    ExplicitFallbackToCpuParallelExact,
    RawGeometryDeterministicSerial,
}

impl SearchBackendSelectionReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AutoCpuGeometryExactCoverBaseline => "auto-cpu-geometry-exact-cover-baseline",
            Self::AutoCpuParallelExact => "auto-cpu-parallel-exact",
            Self::AutoGpuExactConnected => "auto-gpu-exact-connected",
            Self::AutoSmallScenarioCpuGeometryExactCover => {
                "auto-small-scenario-cpu-geometry-exact-cover"
            }
            Self::UserRequested => "user-requested",
            Self::HybridGpuReady => "hybrid-gpu-ready",
            Self::HybridGpuNotReadyCpu => "hybrid-gpu-not-ready-cpu",
            Self::ExplicitFallbackToCpuGeometryExactCover => {
                "explicit-fallback-to-cpu-geometry-exact-cover"
            }
            Self::ExplicitFallbackToCpuParallelExact => "explicit-fallback-to-cpu-parallel-exact",
            Self::RawGeometryDeterministicSerial => "raw-geometry-deterministic-serial",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendSolutionTraceMode {
    None,
}

impl BackendSolutionTraceMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
        }
    }
}
impl BackendSolutionTraceMode {
    pub fn trace_retention_reason(self) -> Option<&'static str> {
        let _ = self;
        None
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchTraversalModel {
    None,
    BitsetAlgorithmX,
    ImmutableGeometryGraphBuildabilityTasks,
    GpuGeometryExactCoverFrontier,
}

impl SearchTraversalModel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::BitsetAlgorithmX => "bitset-algorithm-x",
            Self::ImmutableGeometryGraphBuildabilityTasks => {
                "immutable-geometry-graph-buildability-tasks"
            }
            Self::GpuGeometryExactCoverFrontier => "gpu-geometry-exact-cover-frontier",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComputeDeviceKind {
    None,
    Cpu,
    Gpu,
    Hybrid,
}

impl ComputeDeviceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
            Self::Hybrid => "hybrid",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchBackendFallbackReason {
    GpuFeatureDisabled,
    GpuBindingUnavailable,
    GpuDeviceNotFound,
    GpuKernelUnavailable,
    GpuBackendNotConnected,
    GpuExactSearchUnsupported,
    GpuCapabilityQueryFailed,
    GpuTransientBeforeCommit,
    GpuResourceIncomplete,
}

impl SearchBackendFallbackReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GpuFeatureDisabled => "gpu_feature_disabled",
            Self::GpuBindingUnavailable => "gpu_binding_unavailable",
            Self::GpuDeviceNotFound => "gpu_device_not_found",
            Self::GpuKernelUnavailable => "gpu_kernel_unavailable",
            Self::GpuBackendNotConnected => "gpu_backend_not_connected",
            Self::GpuExactSearchUnsupported => "gpu_exact_search_unsupported",
            Self::GpuCapabilityQueryFailed => "gpu_capability_query_failed",
            Self::GpuTransientBeforeCommit => "gpu_transient_before_commit",
            Self::GpuResourceIncomplete => "gpu_resource_incomplete",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GpuUnavailableReason {
    FeatureDisabled,
    BindingUnavailable,
    DeviceNotFound,
    KernelUnavailable,
    BackendNotConnected,
    ExactSearchUnsupported,
    CapabilityQueryFailed,
}

impl GpuUnavailableReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FeatureDisabled => "gpu_feature_disabled",
            Self::BindingUnavailable => "gpu_binding_unavailable",
            Self::DeviceNotFound => "gpu_device_not_found",
            Self::KernelUnavailable => "gpu_kernel_unavailable",
            Self::BackendNotConnected => "gpu_backend_not_connected",
            Self::ExactSearchUnsupported => "gpu_exact_search_unsupported",
            Self::CapabilityQueryFailed => "gpu_capability_query_failed",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuDeviceSummary {
    requested: String,
    selected_index: Option<u8>,
    selected_name: Option<String>,
    selected_device_type: Option<String>,
    selected_backend: Option<String>,
    selected_vendor: Option<u32>,
    selected_device: Option<u32>,
}

impl GpuDeviceSummary {
    pub(super) fn from_selection(selection: &GpuDeviceSelection) -> Self {
        Self {
            requested: selection.as_display_string(),
            selected_index: None,
            selected_name: None,
            selected_device_type: None,
            selected_backend: None,
            selected_vendor: None,
            selected_device: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    #[cfg(feature = "webgpu-search")]
    pub(crate) fn from_execution(
        selection: &GpuDeviceSelection,
        selected_index: u8,
        selected_name: String,
        selected_device_type: String,
        selected_backend: String,
        selected_vendor: u32,
        selected_device: u32,
    ) -> Self {
        Self {
            requested: selection.as_display_string(),
            selected_index: Some(selected_index),
            selected_name: Some(selected_name),
            selected_device_type: Some(selected_device_type),
            selected_backend: Some(selected_backend),
            selected_vendor: Some(selected_vendor),
            selected_device: Some(selected_device),
        }
    }
}
impl GpuDeviceSummary {
    pub fn requested(&self) -> &str {
        &self.requested
    }

    pub const fn selected_index(&self) -> Option<u8> {
        self.selected_index
    }

    pub fn selected_name(&self) -> Option<&str> {
        self.selected_name.as_deref()
    }

    pub fn selected_device_type(&self) -> Option<&str> {
        self.selected_device_type.as_deref()
    }

    pub fn selected_backend(&self) -> Option<&str> {
        self.selected_backend.as_deref()
    }

    pub const fn selected_vendor(&self) -> Option<u32> {
        self.selected_vendor
    }

    pub const fn selected_device(&self) -> Option<u32> {
        self.selected_device
    }
}
