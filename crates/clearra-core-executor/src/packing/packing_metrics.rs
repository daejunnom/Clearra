use crate::backend::{BackendTrustReport, SelectedSearchBackend};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackingExecutionSource {
    NativeCpuPacking,
    NativeGpuPacking,
    NativeHybridPacking,
}

impl PackingExecutionSource {
    pub fn from_actual_backend(backend: SelectedSearchBackend) -> Self {
        match backend {
            SelectedSearchBackend::Gpu => Self::NativeGpuPacking,
            SelectedSearchBackend::Hybrid => Self::NativeHybridPacking,
            SelectedSearchBackend::None
            | SelectedSearchBackend::CpuGeometryExactCover
            | SelectedSearchBackend::CpuParallelGeometryExactCover => Self::NativeCpuPacking,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::NativeCpuPacking => "native-cpu-packing",
            Self::NativeGpuPacking => "native-gpu-packing",
            Self::NativeHybridPacking => "native-hybrid-packing",
        }
    }
}
impl PackingExecutionSource {
    pub fn candidate_backend(self) -> &'static str {
        match self {
            Self::NativeCpuPacking => "cpu-packing",
            Self::NativeGpuPacking => "gpu-packing",
            Self::NativeHybridPacking => "hybrid-packing",
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuPackingBackendReport {
    backend_scope: &'static str,
    unavailable_reason: &'static str,
    hash_exact_confirm_required: bool,
    larger_batch_planner: bool,
    dominance_prefilter: bool,
    shape_union_mask: bool,
    candidate_hash: &'static str,
    readback_compression: bool,
    cpu_exact_confirm_optimized: bool,
    deterministic_result: bool,
    cpu_reference_confirmed: bool,
    cpu_reference_match: bool,
}

impl GpuPackingBackendReport {
    pub const fn unavailable() -> Self {
        Self {
            backend_scope: "native-gpu-packing",
            unavailable_reason: "native_gpu_backend_not_built",
            hash_exact_confirm_required: true,
            larger_batch_planner: false,
            dominance_prefilter: false,
            shape_union_mask: false,
            candidate_hash: "not-computed",
            readback_compression: false,
            cpu_exact_confirm_optimized: false,
            deterministic_result: false,
            cpu_reference_confirmed: false,
            cpu_reference_match: false,
        }
    }

    pub fn from_execution(
        actual_backend: SelectedSearchBackend,
        trust: BackendTrustReport,
    ) -> Self {
        if !matches!(
            actual_backend,
            SelectedSearchBackend::Gpu | SelectedSearchBackend::Hybrid
        ) {
            return Self::unavailable();
        }

        Self {
            backend_scope: match actual_backend {
                SelectedSearchBackend::Hybrid => "native-hybrid-packing",
                _ => "native-gpu-packing",
            },
            unavailable_reason: "none",
            hash_exact_confirm_required: true,
            larger_batch_planner: false,
            dominance_prefilter: false,
            shape_union_mask: false,
            candidate_hash: "not-exported",
            readback_compression: false,
            cpu_exact_confirm_optimized: false,
            deterministic_result: trust.deterministic_reference_matched(),
            cpu_reference_confirmed: trust.cpu_confirmed(),
            cpu_reference_match: trust.cpu_confirmed(),
        }
    }
}
impl GpuPackingBackendReport {
    pub fn available(self) -> bool {
        self.unavailable_reason == "none"
    }
}
impl GpuPackingBackendReport {
    pub fn backend_scope(self) -> &'static str {
        self.backend_scope
    }
}
impl GpuPackingBackendReport {
    pub fn unavailable_reason(self) -> &'static str {
        self.unavailable_reason
    }
}
impl GpuPackingBackendReport {
    pub fn hash_exact_confirm_required(self) -> bool {
        self.hash_exact_confirm_required
    }
}
impl GpuPackingBackendReport {
    pub fn larger_batch_planner(self) -> bool {
        self.larger_batch_planner
    }
}
impl GpuPackingBackendReport {
    pub fn dominance_prefilter(self) -> bool {
        self.dominance_prefilter
    }
}
impl GpuPackingBackendReport {
    pub fn shape_union_mask(self) -> bool {
        self.shape_union_mask
    }
}
impl GpuPackingBackendReport {
    pub fn candidate_hash(self) -> &'static str {
        self.candidate_hash
    }
}
impl GpuPackingBackendReport {
    pub fn readback_compression(self) -> bool {
        self.readback_compression
    }
}
impl GpuPackingBackendReport {
    pub fn cpu_exact_confirm_optimized(self) -> bool {
        self.cpu_exact_confirm_optimized
    }
}
impl GpuPackingBackendReport {
    pub fn deterministic_result(self) -> bool {
        self.deterministic_result
    }
}
impl GpuPackingBackendReport {
    pub fn cpu_reference_confirmed(self) -> bool {
        self.cpu_reference_confirmed
    }
}
impl GpuPackingBackendReport {
    pub fn cpu_reference_match(self) -> bool {
        self.cpu_reference_match
    }
}
